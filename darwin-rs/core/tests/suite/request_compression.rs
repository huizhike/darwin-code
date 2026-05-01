#![cfg(not(target_os = "windows"))]

use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_darwin_code::test_darwin_code;
use core_test_support::wait_for_event;
use darwin_code_features::Feature;
use darwin_code_protocol::protocol::EventMsg;
use darwin_code_protocol::protocol::Op;
use darwin_code_protocol::user_input::UserInput;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_body_is_not_compressed_for_api_key_auth_even_when_enabled() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let request_log = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp-1"), ev_completed("resp-1")]),
    )
    .await;

    let base_url = format!("{}/backend-api/darwin_code/v1", server.uri());
    let mut builder = test_darwin_code().with_config(move |config| {
        config
            .features
            .enable(Feature::EnableRequestCompression)
            .expect("test config should allow feature update");
        config.model_provider.base_url = Some(base_url);
    });
    let darwin_code = builder.build(&server).await?.darwin_code;

    darwin_code
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "do not compress".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            responsesapi_client_metadata: None,
        })
        .await?;

    wait_for_event(&darwin_code, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let request = request_log.single_request();
    assert!(
        request.header("content-encoding").is_none(),
        "did not expect request compression for API-key auth"
    );

    let json: serde_json::Value = serde_json::from_slice(&request.body_bytes())?;
    assert!(
        json.get("input").is_some(),
        "expected request body to be plain Responses API JSON"
    );

    Ok(())
}
