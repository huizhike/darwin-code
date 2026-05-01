//! BYOK-only MCP file argument handling.
//!
//! Proprietary Apps/Darwin file upload rewriting is intentionally disabled in the
//! local execution engine. External gateways may implement their own file upload
//! adaptation before requests reach DarwinCode.

use crate::session::session::Session;
use crate::session::turn_context::TurnContext;
use serde_json::Value as JsonValue;

pub(crate) async fn rewrite_mcp_tool_arguments_for_darwin_files(
    _sess: &Session,
    _turn_context: &TurnContext,
    arguments_value: Option<JsonValue>,
    _darwin_file_input_params: Option<&[String]>,
) -> Result<Option<JsonValue>, String> {
    Ok(arguments_value)
}
