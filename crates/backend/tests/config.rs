use patchouli_backend::{BackendConfig, ConfigError, PolicySelector};
use serde_json::json;

const SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../config/patchouli.schema.json"
));
const EXAMPLE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../config/patchouli.example.json"
));

#[test]
fn configuration_schema_and_example_are_valid() {
    let schema = serde_json::from_str(SCHEMA).unwrap();
    assert!(jsonschema::meta::is_valid(&schema));

    let config = BackendConfig::from_json(EXAMPLE).unwrap();
    assert_eq!(config.entity_identity.scope_by, ["channel_id"]);
    assert_eq!(
        config.meta_fields["transaction_id"].select(&json!({
            "transaction_id": "transaction-3"
        })),
        Some(&json!("transaction-3"))
    );
}

#[test]
fn configuration_rejects_unknown_properties() {
    let mut value: serde_json::Value = serde_json::from_str(EXAMPLE).unwrap();
    value["unknown"] = json!(true);

    let error = BackendConfig::from_json(&value.to_string()).unwrap_err();
    assert!(matches!(error, ConfigError::Schema { .. }));
}

#[test]
fn configuration_rejects_unknown_field_aliases() {
    let mut value: serde_json::Value = serde_json::from_str(EXAMPLE).unwrap();
    value["entity_types"]["knowledge"]["rules"][0]["behavior"]["baseline"]["key_by"] =
        json!(["missing"]);

    let error = BackendConfig::from_json(&value.to_string()).unwrap_err();
    assert!(matches!(error, ConfigError::Invalid { .. }));
    assert!(error.to_string().contains("unknown metadata field"));
}

#[test]
fn configuration_rejects_invalid_embedded_json_schemas() {
    let mut value: serde_json::Value = serde_json::from_str(EXAMPLE).unwrap();
    value["meta_fields"]["channel_id"]["schema"] = json!({ "type": "identifier" });

    let error = BackendConfig::from_json(&value.to_string()).unwrap_err();
    assert!(matches!(error, ConfigError::Invalid { .. }));
    assert!(error.to_string().contains("valid JSON Schema"));
}

#[test]
fn configuration_rejects_unresolvable_value_schemas() {
    let mut value: serde_json::Value = serde_json::from_str(EXAMPLE).unwrap();
    value["entity_types"]["knowledge"]["value_schema"] =
        json!({ "$ref": "urn:patchouli:schema:missing:1" });

    let error = BackendConfig::from_json(&value.to_string()).unwrap_err();
    assert!(matches!(error, ConfigError::Invalid { .. }));
    assert!(error.to_string().contains("cannot be resolved"));
}

#[test]
fn selector_derives_separate_scope_and_control_keys() {
    let selector = PolicySelector::new(BackendConfig::from_json(EXAMPLE).unwrap());
    let meta = json!({
        "channel_id": "channel-7",
        "transaction_id": "transaction-3",
        "event_time": "2026-08-14T08:00:00Z",
        "plugin_route_id": "route-a",
        "request_id": "request-9",
        "base_versions": ["version-1"]
    });

    let selection = selector.select("knowledge", &meta).unwrap();
    assert_eq!(selection.rule.as_deref(), Some("transaction_batch"));
    assert_eq!(selection.scope, json_map([("channel_id", "channel-7")]));
    assert_eq!(
        selection.baseline_key,
        Some(json_map([
            ("channel_id", "channel-7"),
            ("transaction_id", "transaction-3")
        ]))
    );
    assert_eq!(
        selection.idempotency_key,
        Some(json_map([
            ("channel_id", "channel-7"),
            ("participant_id", "route-a"),
            ("request_id", "request-9"),
            ("transaction_id", "transaction-3")
        ]))
    );
    assert_eq!(selection.publication_key, selection.baseline_key);
}

#[test]
fn disabled_control_features_do_not_create_keys() {
    let selector = PolicySelector::new(BackendConfig::from_json(EXAMPLE).unwrap());
    let meta = json!({ "channel_id": "channel-7" });

    let selection = selector.select("knowledge", &meta).unwrap();
    assert_eq!(selection.rule, None);
    assert_eq!(selection.scope, json_map([("channel_id", "channel-7")]));
    assert_eq!(selection.baseline_key, None);
    assert_eq!(selection.idempotency_key, None);
    assert_eq!(selection.publication_key, None);
}

#[test]
fn entity_scope_may_be_global() {
    let mut value: serde_json::Value = serde_json::from_str(EXAMPLE).unwrap();
    value["entity_identity"]["scope_by"] = json!([]);
    let config = BackendConfig::from_json(&value.to_string()).unwrap();
    let selector = PolicySelector::new(config);

    let selection = selector.select("knowledge", &json!({})).unwrap();
    assert!(selection.scope.is_empty());
    assert_eq!(selection.baseline_key, None);
    assert_eq!(selection.idempotency_key, None);
    assert_eq!(selection.publication_key, None);
}

fn json_map<const N: usize>(
    entries: [(&str, &str); N],
) -> std::collections::BTreeMap<String, serde_json::Value> {
    entries
        .into_iter()
        .map(|(name, value)| (name.to_owned(), json!(value)))
        .collect()
}
