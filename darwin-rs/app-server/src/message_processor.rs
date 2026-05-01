use std::collections::BTreeMap;
use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use crate::byok_connectors as connectors;
use crate::config_api::ConfigApi;
use crate::darwin_code_message_processor::DarwinCodeMessageProcessor;
use crate::darwin_code_message_processor::DarwinCodeMessageProcessorArgs;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::external_agent_config_api::ExternalAgentConfigApi;
use crate::fs_api::FsApi;
use crate::fs_watch::FsWatchManager;
use crate::outgoing_message::ConnectionId;
use crate::outgoing_message::ConnectionRequestId;
use crate::outgoing_message::OutgoingMessageSender;
use crate::outgoing_message::RequestContext;
use crate::transport::AppServerTransport;
use crate::transport::RemoteControlHandle;
use axum::http::HeaderValue;
use darwin_code_analytics::AnalyticsEventsClient;
use darwin_code_analytics::AppServerRpcTransport;
use darwin_code_app_server_protocol::AppListUpdatedNotification;
use darwin_code_app_server_protocol::ClientInfo;
use darwin_code_app_server_protocol::ClientNotification;
use darwin_code_app_server_protocol::ClientRequest;
use darwin_code_app_server_protocol::ConfigBatchWriteParams;
use darwin_code_app_server_protocol::ConfigReadParams;
use darwin_code_app_server_protocol::ConfigValueWriteParams;
use darwin_code_app_server_protocol::ConfigWarningNotification;
use darwin_code_app_server_protocol::ExperimentalApi;
use darwin_code_app_server_protocol::ExperimentalFeatureEnablementSetParams;
use darwin_code_app_server_protocol::ExternalAgentConfigDetectParams;
use darwin_code_app_server_protocol::ExternalAgentConfigImportParams;
use darwin_code_app_server_protocol::ExternalAgentConfigImportResponse;
use darwin_code_app_server_protocol::FsCopyParams;
use darwin_code_app_server_protocol::FsCreateDirectoryParams;
use darwin_code_app_server_protocol::FsGetMetadataParams;
use darwin_code_app_server_protocol::FsReadDirectoryParams;
use darwin_code_app_server_protocol::FsReadFileParams;
use darwin_code_app_server_protocol::FsRemoveParams;
use darwin_code_app_server_protocol::FsUnwatchParams;
use darwin_code_app_server_protocol::FsWatchParams;
use darwin_code_app_server_protocol::FsWriteFileParams;
use darwin_code_app_server_protocol::InitializeResponse;
use darwin_code_app_server_protocol::JSONRPCError;
use darwin_code_app_server_protocol::JSONRPCErrorError;
use darwin_code_app_server_protocol::JSONRPCNotification;
use darwin_code_app_server_protocol::JSONRPCRequest;
use darwin_code_app_server_protocol::JSONRPCResponse;
use darwin_code_app_server_protocol::ServerNotification;
use darwin_code_app_server_protocol::experimental_required_message;
use darwin_code_arg0::Arg0DispatchPaths;
use darwin_code_client::SetOriginatorError;
use darwin_code_client::USER_AGENT_SUFFIX;
use darwin_code_client::get_darwin_code_user_agent;
use darwin_code_client::set_default_client_residency_requirement;
use darwin_code_client::set_default_originator;
use darwin_code_core::ThreadManager;
use darwin_code_core::config::Config;
use darwin_code_core::config_loader::ExternalRequirementsLoader;
use darwin_code_core::config_loader::LoaderOverrides;
use darwin_code_exec_server::EnvironmentManager;
use darwin_code_features::Feature;
use darwin_code_feedback::DarwinCodeFeedback;
use darwin_code_models_manager::collaboration_mode_presets::CollaborationModesConfig;
use darwin_code_protocol::ThreadId;
use darwin_code_protocol::protocol::SessionSource;
use darwin_code_protocol::protocol::W3cTraceContext;
use darwin_code_state::log_db::LogDbLayer;
use futures::FutureExt;
use tokio::sync::broadcast;
use tokio::sync::watch;
use toml::Value as TomlValue;
use tracing::Instrument;

pub(crate) struct MessageProcessor {
    outgoing: Arc<OutgoingMessageSender>,
    darwin_code_message_processor: DarwinCodeMessageProcessor,
    config_api: ConfigApi,
    external_agent_config_api: ExternalAgentConfigApi,
    fs_api: FsApi,
    analytics_events_client: AnalyticsEventsClient,
    fs_watch_manager: FsWatchManager,
    config: Arc<Config>,
    config_warnings: Arc<Vec<ConfigWarningNotification>>,
    rpc_transport: AppServerRpcTransport,
    remote_control_handle: Option<RemoteControlHandle>,
}

#[derive(Debug, Default)]
pub(crate) struct ConnectionSessionState {
    initialized: OnceLock<InitializedConnectionSessionState>,
}

#[derive(Debug)]
struct InitializedConnectionSessionState {
    experimental_api_enabled: bool,
    opted_out_notification_methods: HashSet<String>,
    app_server_client_name: String,
    client_version: String,
}

impl ConnectionSessionState {
    pub(crate) fn initialized(&self) -> bool {
        self.initialized.get().is_some()
    }

    pub(crate) fn experimental_api_enabled(&self) -> bool {
        self.initialized
            .get()
            .is_some_and(|session| session.experimental_api_enabled)
    }

    pub(crate) fn opted_out_notification_methods(&self) -> HashSet<String> {
        self.initialized
            .get()
            .map(|session| session.opted_out_notification_methods.clone())
            .unwrap_or_default()
    }

    pub(crate) fn app_server_client_name(&self) -> Option<&str> {
        self.initialized
            .get()
            .map(|session| session.app_server_client_name.as_str())
    }

    pub(crate) fn client_version(&self) -> Option<&str> {
        self.initialized
            .get()
            .map(|session| session.client_version.as_str())
    }

    fn initialize(&self, session: InitializedConnectionSessionState) -> Result<(), ()> {
        self.initialized.set(session).map_err(|_| ())
    }
}

pub(crate) struct MessageProcessorArgs {
    pub(crate) outgoing: Arc<OutgoingMessageSender>,
    pub(crate) arg0_paths: Arg0DispatchPaths,
    pub(crate) config: Arc<Config>,
    pub(crate) environment_manager: Arc<EnvironmentManager>,
    pub(crate) cli_overrides: Vec<(String, TomlValue)>,
    pub(crate) loader_overrides: LoaderOverrides,
    pub(crate) external_requirements: ExternalRequirementsLoader,
    pub(crate) feedback: DarwinCodeFeedback,
    pub(crate) log_db: Option<LogDbLayer>,
    pub(crate) config_warnings: Vec<ConfigWarningNotification>,
    pub(crate) session_source: SessionSource,
    pub(crate) rpc_transport: AppServerRpcTransport,
    pub(crate) remote_control_handle: Option<RemoteControlHandle>,
}

impl MessageProcessor {
    /// Create a new `MessageProcessor`, retaining a handle to the outgoing
    /// `Sender` so handlers can enqueue messages to be written to stdout.
    pub(crate) fn new(args: MessageProcessorArgs) -> Self {
        let MessageProcessorArgs {
            outgoing,
            arg0_paths,
            config,
            environment_manager,
            cli_overrides,
            loader_overrides,
            external_requirements,
            feedback,
            log_db,
            config_warnings,
            session_source,
            rpc_transport,
            remote_control_handle,
        } = args;
        let analytics_events_client = AnalyticsEventsClient::new(config.analytics_enabled);
        let thread_manager = Arc::new(ThreadManager::new(
            config.as_ref(),
            session_source,
            CollaborationModesConfig {
                default_mode_request_user_input: config
                    .features
                    .enabled(Feature::DefaultModeRequestUserInput),
            },
            environment_manager,
            Some(analytics_events_client.clone()),
        ));
        thread_manager
            .plugins_manager()
            .set_analytics_events_client(analytics_events_client.clone());

        let cli_overrides = Arc::new(RwLock::new(cli_overrides));
        let runtime_feature_enablement = Arc::new(RwLock::new(BTreeMap::new()));
        let external_requirements = Arc::new(RwLock::new(external_requirements));
        let darwin_code_message_processor =
            DarwinCodeMessageProcessor::new(DarwinCodeMessageProcessorArgs {
                thread_manager: Arc::clone(&thread_manager),
                outgoing: outgoing.clone(),
                analytics_events_client: analytics_events_client.clone(),
                arg0_paths,
                config: Arc::clone(&config),
                cli_overrides: cli_overrides.clone(),
                runtime_feature_enablement: runtime_feature_enablement.clone(),
                external_requirements: external_requirements.clone(),
                feedback,
                log_db,
            });
        // Keep plugin startup warmups aligned at app-server startup.
        // TODO(xl): Move into PluginManager once this no longer depends on config feature gating.
        thread_manager
            .plugins_manager()
            .maybe_start_plugin_startup_tasks_for_config(&config);
        let config_api = ConfigApi::new(
            config.darwin_code_home.to_path_buf(),
            cli_overrides,
            runtime_feature_enablement,
            loader_overrides,
            external_requirements,
            thread_manager.clone(),
            analytics_events_client.clone(),
        );
        let external_agent_config_api =
            ExternalAgentConfigApi::new(config.darwin_code_home.to_path_buf());
        let fs_api = FsApi::default();
        let fs_watch_manager = FsWatchManager::new(outgoing.clone());

        Self {
            outgoing,
            darwin_code_message_processor,
            config_api,
            external_agent_config_api,
            fs_api,
            analytics_events_client,
            fs_watch_manager,
            config,
            config_warnings: Arc::new(config_warnings),
            rpc_transport,
            remote_control_handle,
        }
    }

    pub(crate) fn clear_runtime_references(&self) {}

    pub(crate) async fn process_request(
        self: &Arc<Self>,
        connection_id: ConnectionId,
        request: JSONRPCRequest,
        transport: AppServerTransport,
        session: Arc<ConnectionSessionState>,
    ) {
        let request_method = request.method.as_str();
        tracing::trace!(
            ?connection_id,
            request_id = ?request.id,
            "app-server request: {request_method}"
        );
        let request_id = ConnectionRequestId {
            connection_id,
            request_id: request.id.clone(),
        };
        let request_span =
            crate::app_server_tracing::request_span(&request, transport, connection_id, &session);
        let request_trace = request.trace.as_ref().map(|trace| W3cTraceContext {
            traceparent: trace.traceparent.clone(),
            tracestate: trace.tracestate.clone(),
        });
        let request_context = RequestContext::new(request_id.clone(), request_span, request_trace);
        Self::run_request_with_context(
            Arc::clone(&self.outgoing),
            request_context.clone(),
            async {
                let request_json = match serde_json::to_value(&request) {
                    Ok(request_json) => request_json,
                    Err(err) => {
                        let error = JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("Invalid request: {err}"),
                            data: None,
                        };
                        self.outgoing.send_error(request_id.clone(), error).await;
                        return;
                    }
                };

                let darwin_code_request =
                    match serde_json::from_value::<ClientRequest>(request_json) {
                        Ok(darwin_code_request) => darwin_code_request,
                        Err(err) => {
                            let error = JSONRPCErrorError {
                                code: INVALID_REQUEST_ERROR_CODE,
                                message: format!("Invalid request: {err}"),
                                data: None,
                            };
                            self.outgoing.send_error(request_id.clone(), error).await;
                            return;
                        }
                    };
                // Websocket callers finalize outbound readiness in lib.rs after mirroring
                // session state into outbound state and sending initialize notifications to
                // this specific connection. Passing `None` avoids marking the connection
                // ready too early from inside the shared request handler.
                self.handle_client_request(
                    request_id.clone(),
                    darwin_code_request,
                    Arc::clone(&session),
                    /*outbound_initialized*/ None,
                    request_context.clone(),
                )
                .await;
            },
        )
        .await;
    }

    /// Handles a typed request path used by in-process embedders.
    ///
    /// This bypasses JSON request deserialization but keeps identical request
    /// semantics by delegating to `handle_client_request`.
    pub(crate) async fn process_client_request(
        self: &Arc<Self>,
        connection_id: ConnectionId,
        request: ClientRequest,
        session: Arc<ConnectionSessionState>,
        outbound_initialized: &AtomicBool,
    ) {
        let request_id = ConnectionRequestId {
            connection_id,
            request_id: request.id().clone(),
        };
        let request_span =
            crate::app_server_tracing::typed_request_span(&request, connection_id, &session);
        let request_context =
            RequestContext::new(request_id.clone(), request_span, /*parent_trace*/ None);
        tracing::trace!(
            ?connection_id,
            request_id = ?request_id.request_id,
            "app-server typed request"
        );
        Self::run_request_with_context(
            Arc::clone(&self.outgoing),
            request_context.clone(),
            async {
                // In-process clients do not have the websocket transport loop that performs
                // post-initialize bookkeeping, so they still finalize outbound readiness in
                // the shared request handler.
                self.handle_client_request(
                    request_id.clone(),
                    request,
                    Arc::clone(&session),
                    Some(outbound_initialized),
                    request_context.clone(),
                )
                .await;
            },
        )
        .await;
    }

    pub(crate) async fn process_notification(&self, notification: JSONRPCNotification) {
        // Currently, we do not expect to receive any notifications from the
        // client, so we just log them.
        tracing::info!("<- notification: {:?}", notification);
    }

    /// Handles typed notifications from in-process clients.
    pub(crate) async fn process_client_notification(&self, notification: ClientNotification) {
        // Currently, we do not expect to receive any typed notifications from
        // in-process clients, so we just log them.
        tracing::info!("<- typed notification: {:?}", notification);
    }

    async fn run_request_with_context<F>(
        outgoing: Arc<OutgoingMessageSender>,
        request_context: RequestContext,
        request_fut: F,
    ) where
        F: Future<Output = ()>,
    {
        outgoing
            .register_request_context(request_context.clone())
            .await;
        request_fut.instrument(request_context.span()).await;
    }

    pub(crate) fn thread_created_receiver(&self) -> broadcast::Receiver<ThreadId> {
        self.darwin_code_message_processor.thread_created_receiver()
    }

    pub(crate) async fn send_initialize_notifications_to_connection(
        &self,
        connection_id: ConnectionId,
    ) {
        for notification in self.config_warnings.iter().cloned() {
            self.outgoing
                .send_server_notification_to_connections(
                    &[connection_id],
                    ServerNotification::ConfigWarning(notification),
                )
                .await;
        }
    }

    pub(crate) async fn connection_initialized(&self, connection_id: ConnectionId) {
        self.darwin_code_message_processor
            .connection_initialized(connection_id)
            .await;
    }

    pub(crate) async fn send_initialize_notifications(&self) {
        for notification in self.config_warnings.iter().cloned() {
            self.outgoing
                .send_server_notification(ServerNotification::ConfigWarning(notification))
                .await;
        }
    }

    pub(crate) async fn try_attach_thread_listener(
        &self,
        thread_id: ThreadId,
        connection_ids: Vec<ConnectionId>,
    ) {
        self.darwin_code_message_processor
            .try_attach_thread_listener(thread_id, connection_ids)
            .await;
    }

    pub(crate) async fn drain_background_tasks(&self) {
        self.darwin_code_message_processor
            .drain_background_tasks()
            .await;
    }

    pub(crate) async fn clear_all_thread_listeners(&self) {
        self.darwin_code_message_processor
            .clear_all_thread_listeners()
            .await;
    }

    pub(crate) async fn shutdown_threads(&self) {
        self.darwin_code_message_processor.shutdown_threads().await;
    }

    pub(crate) async fn connection_closed(&self, connection_id: ConnectionId) {
        self.outgoing.connection_closed(connection_id).await;
        self.fs_watch_manager.connection_closed(connection_id).await;
        self.darwin_code_message_processor
            .connection_closed(connection_id)
            .await;
    }

    pub(crate) fn subscribe_running_assistant_turn_count(&self) -> watch::Receiver<usize> {
        self.darwin_code_message_processor
            .subscribe_running_assistant_turn_count()
    }

    /// Handle a standalone JSON-RPC response originating from the peer.
    pub(crate) async fn process_response(&self, response: JSONRPCResponse) {
        tracing::info!("<- response: {:?}", response);
        let JSONRPCResponse { id, result, .. } = response;
        self.outgoing.notify_client_response(id, result).await
    }

    /// Handle an error object received from the peer.
    pub(crate) async fn process_error(&self, err: JSONRPCError) {
        tracing::error!("<- error: {:?}", err);
        self.outgoing.notify_client_error(err.id, err.error).await;
    }

    async fn handle_client_request(
        self: &Arc<Self>,
        connection_request_id: ConnectionRequestId,
        darwin_code_request: ClientRequest,
        session: Arc<ConnectionSessionState>,
        // `Some(...)` means the caller wants initialize to immediately mark the
        // connection outbound-ready. Websocket JSON-RPC calls pass `None` so
        // lib.rs can deliver connection-scoped initialize notifications first.
        outbound_initialized: Option<&AtomicBool>,
        request_context: RequestContext,
    ) {
        let connection_id = connection_request_id.connection_id;
        if let ClientRequest::Initialize { request_id, params } = darwin_code_request {
            // Handle Initialize internally so DarwinCodeMessageProcessor does not have to concern
            // itself with the `initialized` bool.
            let connection_request_id = ConnectionRequestId {
                connection_id,
                request_id,
            };
            if session.initialized() {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: "Already initialized".to_string(),
                    data: None,
                };
                self.outgoing.send_error(connection_request_id, error).await;
                return;
            }

            // TODO(maxj): Revisit capability scoping for `experimental_api_enabled`.
            // Current behavior is per-connection. Reviewer feedback notes this can
            // create odd cross-client behavior (for example dynamic tool calls on a
            // shared thread when another connected client did not opt into
            // experimental API). Proposed direction is instance-global first-write-wins
            // with initialize-time mismatch rejection.
            let analytics_initialize_params = params.clone();
            let (experimental_api_enabled, opt_out_notification_methods) = match params.capabilities
            {
                Some(capabilities) => (
                    capabilities.experimental_api,
                    capabilities
                        .opt_out_notification_methods
                        .unwrap_or_default(),
                ),
                None => (false, Vec::new()),
            };
            let ClientInfo {
                name,
                title: _title,
                version,
            } = params.client_info;
            // Validate before committing; set_default_originator validates while
            // mutating process-global metadata.
            if HeaderValue::from_str(&name).is_err() {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: format!(
                        "Invalid clientInfo.name: '{name}'. Must be a valid HTTP header value."
                    ),
                    data: None,
                };
                self.outgoing
                    .send_error(connection_request_id.clone(), error)
                    .await;
                return;
            }
            let originator = name.clone();
            let user_agent_suffix = format!("{name}; {version}");
            let darwin_code_home = self.config.darwin_code_home.clone();
            if session
                .initialize(InitializedConnectionSessionState {
                    experimental_api_enabled,
                    opted_out_notification_methods: opt_out_notification_methods
                        .into_iter()
                        .collect(),
                    app_server_client_name: name.clone(),
                    client_version: version,
                })
                .is_err()
            {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: "Already initialized".to_string(),
                    data: None,
                };
                self.outgoing.send_error(connection_request_id, error).await;
                return;
            }

            // Only the request that wins session initialization may mutate
            // process-global client metadata.
            if let Err(error) = set_default_originator(originator.clone()) {
                match error {
                    SetOriginatorError::InvalidHeaderValue => {
                        tracing::warn!(
                            client_info_name = %name,
                            "validated clientInfo.name was rejected while setting originator"
                        );
                    }
                    SetOriginatorError::AlreadyInitialized => {
                        // No-op. This is expected to happen if the originator is already set via env var.
                        // TODO(owen): Once we remove support for DARWIN_CODE_INTERNAL_ORIGINATOR_OVERRIDE,
                        // this will be an unexpected state and we can return a JSON-RPC error indicating
                        // internal server error.
                    }
                }
            }
            if self.config.features.enabled(Feature::GeneralAnalytics) {
                self.analytics_events_client.track_initialize(
                    connection_id.0,
                    analytics_initialize_params,
                    originator,
                    self.rpc_transport,
                );
            }
            set_default_client_residency_requirement(self.config.enforce_residency.value());
            if let Ok(mut suffix) = USER_AGENT_SUFFIX.lock() {
                *suffix = Some(user_agent_suffix);
            }

            let user_agent = get_darwin_code_user_agent();
            let response = InitializeResponse {
                user_agent,
                darwin_code_home: darwin_code_home,
                platform_family: std::env::consts::FAMILY.to_string(),
                platform_os: std::env::consts::OS.to_string(),
            };

            self.outgoing
                .send_response(connection_request_id, response)
                .await;

            if let Some(outbound_initialized) = outbound_initialized {
                // In-process clients can complete readiness immediately here. The
                // websocket path defers this until lib.rs finishes transport-layer
                // initialize handling for the specific connection.
                outbound_initialized.store(true, Ordering::Release);
                self.darwin_code_message_processor
                    .connection_initialized(connection_id)
                    .await;
            }
            return;
        }

        self.dispatch_initialized_client_request(
            connection_request_id,
            darwin_code_request,
            session,
            request_context,
        )
        .await;
    }

    async fn dispatch_initialized_client_request(
        self: &Arc<Self>,
        connection_request_id: ConnectionRequestId,
        darwin_code_request: ClientRequest,
        session: Arc<ConnectionSessionState>,
        request_context: RequestContext,
    ) {
        if !session.initialized() {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "Not initialized".to_string(),
                data: None,
            };
            self.outgoing.send_error(connection_request_id, error).await;
            return;
        }

        if let Some(reason) = darwin_code_request.experimental_reason()
            && !session.experimental_api_enabled()
        {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: experimental_required_message(reason),
                data: None,
            };
            self.outgoing.send_error(connection_request_id, error).await;
            return;
        }
        let connection_id = connection_request_id.connection_id;
        if self.config.features.enabled(Feature::GeneralAnalytics)
            && let ClientRequest::TurnStart { request_id, .. }
            | ClientRequest::TurnSteer { request_id, .. } = &darwin_code_request
        {
            self.analytics_events_client.track_request(
                connection_id.0,
                request_id.clone(),
                darwin_code_request.clone(),
            );
        }

        let app_server_client_name = session.app_server_client_name().map(str::to_string);
        let client_version = session.client_version().map(str::to_string);
        Arc::clone(self)
            .handle_initialized_client_request(
                connection_request_id,
                darwin_code_request,
                request_context,
                app_server_client_name,
                client_version,
            )
            .await;
    }

    async fn handle_initialized_client_request(
        self: Arc<Self>,
        connection_request_id: ConnectionRequestId,
        darwin_code_request: ClientRequest,
        request_context: RequestContext,
        app_server_client_name: Option<String>,
        client_version: Option<String>,
    ) {
        let connection_id = connection_request_id.connection_id;

        match darwin_code_request {
            ClientRequest::ConfigRead { request_id, params } => {
                self.handle_config_read(
                    ConnectionRequestId {
                        connection_id,
                        request_id,
                    },
                    params,
                )
                .await;
            }
            ClientRequest::ExternalAgentConfigDetect { request_id, params } => {
                self.handle_external_agent_config_detect(
                    ConnectionRequestId {
                        connection_id,
                        request_id,
                    },
                    params,
                )
                .await;
            }
            ClientRequest::ExternalAgentConfigImport { request_id, params } => {
                self.handle_external_agent_config_import(
                    ConnectionRequestId {
                        connection_id,
                        request_id,
                    },
                    params,
                )
                .await;
            }
            ClientRequest::ConfigValueWrite { request_id, params } => {
                self.handle_config_value_write(
                    ConnectionRequestId {
                        connection_id,
                        request_id,
                    },
                    params,
                )
                .await;
            }
            ClientRequest::ConfigBatchWrite { request_id, params } => {
                self.handle_config_batch_write(
                    ConnectionRequestId {
                        connection_id,
                        request_id,
                    },
                    params,
                )
                .await;
            }
            ClientRequest::ExperimentalFeatureEnablementSet { request_id, params } => {
                self.handle_experimental_feature_enablement_set(
                    ConnectionRequestId {
                        connection_id,
                        request_id,
                    },
                    params,
                )
                .await;
            }

            ClientRequest::FsReadFile { request_id, params } => {
                self.handle_fs_read_file(
                    ConnectionRequestId {
                        connection_id,
                        request_id,
                    },
                    params,
                )
                .await;
            }
            ClientRequest::FsWriteFile { request_id, params } => {
                self.handle_fs_write_file(
                    ConnectionRequestId {
                        connection_id,
                        request_id,
                    },
                    params,
                )
                .await;
            }
            ClientRequest::FsCreateDirectory { request_id, params } => {
                self.handle_fs_create_directory(
                    ConnectionRequestId {
                        connection_id,
                        request_id,
                    },
                    params,
                )
                .await;
            }
            ClientRequest::FsGetMetadata { request_id, params } => {
                self.handle_fs_get_metadata(
                    ConnectionRequestId {
                        connection_id,
                        request_id,
                    },
                    params,
                )
                .await;
            }
            ClientRequest::FsReadDirectory { request_id, params } => {
                self.handle_fs_read_directory(
                    ConnectionRequestId {
                        connection_id,
                        request_id,
                    },
                    params,
                )
                .await;
            }
            ClientRequest::FsRemove { request_id, params } => {
                self.handle_fs_remove(
                    ConnectionRequestId {
                        connection_id,
                        request_id,
                    },
                    params,
                )
                .await;
            }
            ClientRequest::FsCopy { request_id, params } => {
                self.handle_fs_copy(
                    ConnectionRequestId {
                        connection_id,
                        request_id,
                    },
                    params,
                )
                .await;
            }
            ClientRequest::FsWatch { request_id, params } => {
                self.handle_fs_watch(
                    ConnectionRequestId {
                        connection_id,
                        request_id,
                    },
                    connection_id,
                    params,
                )
                .await;
            }
            ClientRequest::FsUnwatch { request_id, params } => {
                self.handle_fs_unwatch(
                    ConnectionRequestId {
                        connection_id,
                        request_id,
                    },
                    connection_id,
                    params,
                )
                .await;
            }
            other => {
                // Box the delegated future so this wrapper's async state machine does not
                // inline the full `DarwinCodeMessageProcessor::process_request` future, which
                // can otherwise push worker-thread stack usage over the edge.
                self.darwin_code_message_processor
                    .process_request(
                        connection_id,
                        other,
                        app_server_client_name,
                        client_version,
                        request_context,
                    )
                    .boxed()
                    .await;
            }
        }
    }

    async fn handle_config_read(&self, request_id: ConnectionRequestId, params: ConfigReadParams) {
        match self.config_api.read(params).await {
            Ok(response) => self.outgoing.send_response(request_id, response).await,
            Err(error) => self.outgoing.send_error(request_id, error).await,
        }
    }

    async fn handle_config_value_write(
        &self,
        request_id: ConnectionRequestId,
        params: ConfigValueWriteParams,
    ) {
        let result = self.config_api.write_value(params).await;
        self.handle_config_mutation_result(request_id, result).await
    }

    async fn handle_config_batch_write(
        &self,
        request_id: ConnectionRequestId,
        params: ConfigBatchWriteParams,
    ) {
        let result = self.config_api.batch_write(params).await;
        self.handle_config_mutation_result(request_id, result).await;
    }

    async fn handle_experimental_feature_enablement_set(
        &self,
        request_id: ConnectionRequestId,
        params: ExperimentalFeatureEnablementSetParams,
    ) {
        let should_refresh_apps_list = params.enablement.get("apps").copied() == Some(true);
        let result = self
            .config_api
            .set_experimental_feature_enablement(params)
            .await;
        let is_ok = result.is_ok();
        self.handle_config_mutation_result(request_id, result).await;
        if should_refresh_apps_list && is_ok {
            self.refresh_apps_list_after_experimental_feature_enablement_set()
                .await;
        }
    }

    async fn refresh_apps_list_after_experimental_feature_enablement_set(&self) {
        let config = match self
            .config_api
            .load_latest_config(/*fallback_cwd*/ None)
            .await
        {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(
                    "failed to load config for apps list refresh after experimental feature enablement: {}",
                    error.message
                );
                return;
            }
        };
        if !config.features.apps_enabled() {
            return;
        }

        let outgoing = Arc::clone(&self.outgoing);
        tokio::spawn(async move {
            let (all_connectors_result, accessible_connectors_result) = tokio::join!(
                connectors::list_all_connectors_with_options(&config, /*force_refetch*/ true),
                connectors::list_accessible_connectors_from_mcp_tools_with_options(
                    &config, /*force_refetch*/ true,
                ),
            );
            let all_connectors = match all_connectors_result {
                Ok(connectors) => connectors,
                Err(err) => {
                    tracing::warn!(
                        "failed to force-refresh directory apps after experimental feature enablement: {err:#}"
                    );
                    return;
                }
            };
            let accessible_connectors = match accessible_connectors_result {
                Ok(connectors) => connectors,
                Err(err) => {
                    tracing::warn!(
                        "failed to force-refresh accessible apps after experimental feature enablement: {err:#}"
                    );
                    return;
                }
            };

            let data = connectors::with_app_enabled_state(
                connectors::merge_connectors_with_accessible(
                    all_connectors,
                    accessible_connectors,
                    /*all_connectors_loaded*/ true,
                ),
                &config,
            );
            outgoing
                .send_server_notification(ServerNotification::AppListUpdated(
                    AppListUpdatedNotification { data },
                ))
                .await;
        });
    }

    async fn handle_config_mutation_result<T: serde::Serialize>(
        &self,
        request_id: ConnectionRequestId,
        result: std::result::Result<T, JSONRPCErrorError>,
    ) {
        match result {
            Ok(response) => {
                self.handle_config_mutation().await;
                self.outgoing.send_response(request_id, response).await;
            }
            Err(error) => self.outgoing.send_error(request_id, error).await,
        }
    }

    async fn handle_config_mutation(&self) {
        self.darwin_code_message_processor.handle_config_mutation();
        let Some(remote_control_handle) = &self.remote_control_handle else {
            return;
        };

        match self
            .config_api
            .load_latest_config(/*fallback_cwd*/ None)
            .await
        {
            Ok(config) => {
                remote_control_handle.set_enabled(config.features.enabled(Feature::RemoteControl));
            }
            Err(error) => {
                tracing::warn!(
                    "failed to load config for remote control enablement refresh after config mutation: {}",
                    error.message
                );
            }
        }
    }

    async fn handle_external_agent_config_detect(
        &self,
        request_id: ConnectionRequestId,
        params: ExternalAgentConfigDetectParams,
    ) {
        match self.external_agent_config_api.detect(params).await {
            Ok(response) => self.outgoing.send_response(request_id, response).await,
            Err(error) => self.outgoing.send_error(request_id, error).await,
        }
    }

    async fn handle_external_agent_config_import(
        &self,
        request_id: ConnectionRequestId,
        params: ExternalAgentConfigImportParams,
    ) {
        match self.external_agent_config_api.import(params).await {
            Ok(()) => {
                self.handle_config_mutation().await;
                self.outgoing
                    .send_response(request_id, ExternalAgentConfigImportResponse {})
                    .await;
            }
            Err(error) => self.outgoing.send_error(request_id, error).await,
        }
    }

    async fn handle_fs_read_file(&self, request_id: ConnectionRequestId, params: FsReadFileParams) {
        match self.fs_api.read_file(params).await {
            Ok(response) => self.outgoing.send_response(request_id, response).await,
            Err(error) => self.outgoing.send_error(request_id, error).await,
        }
    }

    async fn handle_fs_write_file(
        &self,
        request_id: ConnectionRequestId,
        params: FsWriteFileParams,
    ) {
        match self.fs_api.write_file(params).await {
            Ok(response) => self.outgoing.send_response(request_id, response).await,
            Err(error) => self.outgoing.send_error(request_id, error).await,
        }
    }

    async fn handle_fs_create_directory(
        &self,
        request_id: ConnectionRequestId,
        params: FsCreateDirectoryParams,
    ) {
        match self.fs_api.create_directory(params).await {
            Ok(response) => self.outgoing.send_response(request_id, response).await,
            Err(error) => self.outgoing.send_error(request_id, error).await,
        }
    }

    async fn handle_fs_get_metadata(
        &self,
        request_id: ConnectionRequestId,
        params: FsGetMetadataParams,
    ) {
        match self.fs_api.get_metadata(params).await {
            Ok(response) => self.outgoing.send_response(request_id, response).await,
            Err(error) => self.outgoing.send_error(request_id, error).await,
        }
    }

    async fn handle_fs_read_directory(
        &self,
        request_id: ConnectionRequestId,
        params: FsReadDirectoryParams,
    ) {
        match self.fs_api.read_directory(params).await {
            Ok(response) => self.outgoing.send_response(request_id, response).await,
            Err(error) => self.outgoing.send_error(request_id, error).await,
        }
    }

    async fn handle_fs_remove(&self, request_id: ConnectionRequestId, params: FsRemoveParams) {
        match self.fs_api.remove(params).await {
            Ok(response) => self.outgoing.send_response(request_id, response).await,
            Err(error) => self.outgoing.send_error(request_id, error).await,
        }
    }

    async fn handle_fs_copy(&self, request_id: ConnectionRequestId, params: FsCopyParams) {
        match self.fs_api.copy(params).await {
            Ok(response) => self.outgoing.send_response(request_id, response).await,
            Err(error) => self.outgoing.send_error(request_id, error).await,
        }
    }

    async fn handle_fs_watch(
        &self,
        request_id: ConnectionRequestId,
        connection_id: ConnectionId,
        params: FsWatchParams,
    ) {
        match self.fs_watch_manager.watch(connection_id, params).await {
            Ok(response) => self.outgoing.send_response(request_id, response).await,
            Err(error) => self.outgoing.send_error(request_id, error).await,
        }
    }

    async fn handle_fs_unwatch(
        &self,
        request_id: ConnectionRequestId,
        connection_id: ConnectionId,
        params: FsUnwatchParams,
    ) {
        match self.fs_watch_manager.unwatch(connection_id, params).await {
            Ok(response) => self.outgoing.send_response(request_id, response).await,
            Err(error) => self.outgoing.send_error(request_id, error).await,
        }
    }
}

#[cfg(test)]
mod tracing_tests;
