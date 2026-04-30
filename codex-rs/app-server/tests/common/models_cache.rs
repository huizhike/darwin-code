use chrono::DateTime;
use chrono::Utc;
use darwin_code_models_manager::bundled_models_response;
use darwin_code_models_manager::client_version_to_whole;
use darwin_code_protocol::openai_models::ModelInfo;
use darwin_code_protocol::openai_models::ModelVisibility;
use serde_json::json;
use std::path::Path;

/// Write a models_cache.json file to the darwin_code home directory.
/// This prevents ModelsManager from making network requests to refresh models.
/// The cache will be treated as fresh (within TTL) and used instead of fetching from the network.
/// Uses bundled ModelInfo entries so instruction templates and personality metadata remain intact.
pub fn write_models_cache(darwin_code_home: &Path) -> std::io::Result<()> {
    let mut models = bundled_models_response()
        .map_err(std::io::Error::other)?
        .models
        .into_iter()
        .filter(|model| model.visibility == ModelVisibility::List)
        .collect::<Vec<_>>();
    models.sort_by(|a, b| a.priority.cmp(&b.priority));

    write_models_cache_with_models(darwin_code_home, models)
}

/// Write a models_cache.json file with specific models.
/// Useful when tests need specific models to be available.
pub fn write_models_cache_with_models(
    darwin_code_home: &Path,
    models: Vec<ModelInfo>,
) -> std::io::Result<()> {
    let cache_path = darwin_code_home.join("models_cache.json");
    // DateTime<Utc> serializes to RFC3339 format by default with serde
    let fetched_at: DateTime<Utc> = Utc::now();
    let client_version = client_version_to_whole();
    let cache = json!({
        "fetched_at": fetched_at,
        "etag": null,
        "client_version": client_version,
        "models": models
    });
    std::fs::write(cache_path, serde_json::to_string_pretty(&cache)?)
}
