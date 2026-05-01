use anyhow::Context;
use anyhow::Result;
use darwin_code_app_server_protocol::generate_json_with_experimental;
use darwin_code_app_server_protocol::generate_typescript_schema_fixture_subtree_for_tests;
use darwin_code_app_server_protocol::read_schema_fixture_subtree;
use std::path::Path;

#[test]
fn typescript_schema_generation_smoke() -> Result<()> {
    let generated_tree = generate_typescript_schema_fixture_subtree_for_tests()
        .context("generate in-memory typescript schema fixtures")?;

    assert!(
        generated_tree.contains_key(Path::new("index.ts")),
        "generated TypeScript schema should include index.ts"
    );
    assert!(
        generated_tree.contains_key(Path::new("ClientRequest.ts")),
        "generated TypeScript schema should include ClientRequest.ts"
    );
    assert!(
        generated_tree.contains_key(Path::new("v2/TurnError.ts")),
        "generated TypeScript schema should include v2/TurnError.ts"
    );
    assert!(
        !generated_tree.contains_key(Path::new("v2/MockExperimentalMethodParams.ts")),
        "default TypeScript schema generation should omit experimental entries"
    );

    let index = std::str::from_utf8(
        generated_tree
            .get(Path::new("index.ts"))
            .expect("index.ts presence checked above"),
    )?;
    assert!(index.contains("export type { ClientRequest }"));
    assert!(index.contains("export type { ServerNotification }"));

    Ok(())
}

#[test]
fn json_schema_generation_smoke() -> Result<()> {
    let temp_dir = tempfile::tempdir().context("create temp dir")?;
    let generated_root = temp_dir.path().join("json");
    generate_json_with_experimental(&generated_root, /*experimental_api*/ false).with_context(
        || {
            format!(
                "generate json schema fixtures into {}",
                generated_root.display()
            )
        },
    )?;

    let generated_tree = read_schema_fixture_subtree(temp_dir.path(), "json")
        .context("read generated json schema tree")?;
    for required_path in [
        "ClientRequest.json",
        "ServerNotification.json",
        "darwin_code_app_server_protocol.schemas.json",
        "darwin_code_app_server_protocol.v2.schemas.json",
        "v2/ExperimentalFeatureEnablementSetResponse.json",
        "v2/TurnCompletedNotification.json",
    ] {
        assert!(
            generated_tree.contains_key(Path::new(required_path)),
            "generated JSON schema should include {required_path}"
        );
    }
    assert!(
        !generated_tree.contains_key(Path::new("v2/MockExperimentalMethodParams.json")),
        "default JSON schema generation should omit experimental entries"
    );

    Ok(())
}
