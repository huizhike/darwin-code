//! Implements the MultiAgentV2 collaboration tool surface.

use crate::agent::AgentStatus;
use crate::agent::agent_resolver::resolve_agent_target;
use crate::agent::exceeds_thread_spawn_depth_limit;
use crate::function_tool::FunctionCallError;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::multi_agents_common::*;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use darwin_code_protocol::AgentPath;
use darwin_code_protocol::model_metadata::ReasoningEffort;
use darwin_code_protocol::models::ResponseInputItem;
use darwin_code_protocol::protocol::CollabAgentInteractionBeginEvent;
use darwin_code_protocol::protocol::CollabAgentInteractionEndEvent;
use darwin_code_protocol::protocol::CollabAgentSpawnBeginEvent;
use darwin_code_protocol::protocol::CollabAgentSpawnEndEvent;
use darwin_code_protocol::protocol::CollabCloseBeginEvent;
use darwin_code_protocol::protocol::CollabCloseEndEvent;
use darwin_code_protocol::protocol::CollabWaitingBeginEvent;
use darwin_code_protocol::protocol::CollabWaitingEndEvent;
use darwin_code_protocol::user_input::UserInput;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;

pub(crate) use close_agent::Handler as CloseAgentHandler;
pub(crate) use followup_task::Handler as FollowupTaskHandler;
pub(crate) use list_agents::Handler as ListAgentsHandler;
pub(crate) use send_message::Handler as SendMessageHandler;
pub(crate) use spawn::Handler as SpawnAgentHandler;
pub(crate) use wait::Handler as WaitAgentHandler;

mod close_agent;
mod followup_task;
mod list_agents;
mod message_tool;
mod send_message;
mod spawn;
pub(crate) mod wait;
