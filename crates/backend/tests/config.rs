use patchouli_backend::{BackendConfig, ConfigError, PolicyEngine};
use serde_json::json;

const EXAMPLE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../config/patchouli.example.json"
));

#[test]
fn example_configuration_is_valid() {
    let config = BackendConfig::from_json(EXAMPLE).unwrap();
    let event = &config.entity_types["event"];

    let operation = json!({
        "meta": {
            "channel_id": "channel-7",
            "transaction_id": "transaction-3"
        },
        "data": {
            "type": "event",
            "id": "event-42",
            "value": {}
        }
    });
    assert_eq!(
        event.fields["transaction_id"].select(&operation),
        Some(&json!("transaction-3"))
    );
}

#[test]
fn configuration_rejects_unknown_field_aliases() {
    let mut value: serde_json::Value = serde_json::from_str(EXAMPLE).unwrap();
    value["entity_types"]["event"]["consistency"]["fallback"]["identity"] = json!(["missing"]);

    let error = BackendConfig::from_json(&value.to_string()).unwrap_err();
    assert!(matches!(error, ConfigError::Invalid { .. }));
    assert!(error.to_string().contains("unknown field alias"));
}

#[test]
fn controller_selects_policy_from_configured_fields() {
    let engine = PolicyEngine::new(BackendConfig::from_json(EXAMPLE).unwrap());
    let operation = json!({
        "meta": {
            "channel_id": "channel-7",
            "transaction_id": "transaction-3",
            "event_time": "2026-08-14T08:00:00Z",
            "plugin_route_id": "route-a"
        },
        "data": {
            "type": "event",
            "id": "event-42",
            "value": { "payload": {} }
        }
    });

    let decision = engine.decide("event", &operation).unwrap();
    assert_eq!(decision.rule.as_deref(), Some("transaction_batch"));
    assert_eq!(decision.group["transaction_id"], "transaction-3");
    assert_eq!(decision.identity["participant_id"], "route-a");
}
