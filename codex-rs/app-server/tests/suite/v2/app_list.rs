use anyhow::Result;
use app_test_support::McpProcess;
use app_test_support::to_response;
use darwin_code_app_server_protocol::AppsListParams;
use darwin_code_app_server_protocol::AppsListResponse;
use darwin_code_app_server_protocol::RequestId;
use tempfile::TempDir;

#[tokio::test]
async fn list_apps_returns_empty_in_byok_mode() -> Result<()> {
    let darwin_code_home = TempDir::new()?;
    let mut mcp = McpProcess::new(darwin_code_home.path()).await?;
    mcp.initialize().await?;

    let request_id = mcp
        .send_apps_list_request(AppsListParams {
            cursor: None,
            limit: None,
            thread_id: None,
            force_refetch: false,
        })
        .await?;
    let response = mcp
        .read_stream_until_response_message(RequestId::Integer(request_id))
        .await?;
    let AppsListResponse { data, next_cursor } = to_response(response)?;

    assert!(data.is_empty());
    assert!(next_cursor.is_none());
    Ok(())
}

#[tokio::test]
async fn list_apps_returns_empty_with_pagination_inputs_in_byok_mode() -> Result<()> {
    let darwin_code_home = TempDir::new()?;
    let mut mcp = McpProcess::new(darwin_code_home.path()).await?;
    mcp.initialize().await?;

    let request_id = mcp
        .send_apps_list_request(AppsListParams {
            cursor: Some("0".to_string()),
            limit: Some(1),
            thread_id: None,
            force_refetch: true,
        })
        .await?;
    let response = mcp
        .read_stream_until_response_message(RequestId::Integer(request_id))
        .await?;
    let AppsListResponse { data, next_cursor } = to_response(response)?;

    assert!(data.is_empty());
    assert!(next_cursor.is_none());
    Ok(())
}
