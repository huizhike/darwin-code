use super::*;
use crate::ModelsManagerConfig;
use pretty_assertions::assert_eq;

#[test]
fn reasoning_summaries_override_true_enables_support() {
    let model = model_info_from_slug("unknown-model");
    let config = ModelsManagerConfig {
        model_supports_reasoning_summaries: Some(true),
        ..Default::default()
    };

    let updated = with_config_overrides(model.clone(), &config);
    let mut expected = model;
    expected.supports_reasoning_summaries = true;

    assert_eq!(updated, expected);
}

#[test]
fn reasoning_summaries_override_false_does_not_disable_support() {
    let mut model = model_info_from_slug("unknown-model");
    model.supports_reasoning_summaries = true;
    let config = ModelsManagerConfig {
        model_supports_reasoning_summaries: Some(false),
        ..Default::default()
    };

    let updated = with_config_overrides(model.clone(), &config);

    assert_eq!(updated, model);
}

#[test]
fn reasoning_summaries_override_false_is_noop_when_model_is_false() {
    let model = model_info_from_slug("unknown-model");
    let config = ModelsManagerConfig {
        model_supports_reasoning_summaries: Some(false),
        ..Default::default()
    };

    let updated = with_config_overrides(model.clone(), &config);

    assert_eq!(updated, model);
}

#[test]
fn context_window_override_updates_auto_compact_model_info() {
    let model = model_info_from_slug("unknown-model");
    let config = ModelsManagerConfig {
        model_context_window: Some(100_000),
        ..Default::default()
    };

    let updated = with_config_overrides(model, &config);

    assert_eq!(updated.context_window, Some(100_000));
    assert_eq!(updated.auto_compact_token_limit(), Some(90_000));
}

#[test]
fn auto_compact_override_updates_model_info() {
    let model = model_info_from_slug("unknown-model");
    let config = ModelsManagerConfig {
        model_auto_compact_token_limit: Some(50_000),
        ..Default::default()
    };

    let updated = with_config_overrides(model, &config);

    assert_eq!(updated.auto_compact_token_limit, Some(50_000));
    assert_eq!(updated.auto_compact_token_limit(), Some(50_000));
}
