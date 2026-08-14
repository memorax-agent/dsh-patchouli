use patchouli_backend::{
    BackendConfig, CausalConsistencyPlan, ConfigError, ConflictStrategy, ConsistencySource,
    ControlKey, PolicySelector, PublicationPolicy, SessionGuarantee, SnapshotPolicy,
};
use serde_json::json;

const SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../config/patchouli.schema.json"
));
const EXAMPLE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../config/patchouli.example.json"
));
const DEFAULT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../config/patchouli.default.json"
));
const EVENTUAL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../config/patterns/eventual.json"
));
const CAUSAL_SESSION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../config/patterns/causal_session.json"
));
const SHARED_TRANSACTION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../config/patterns/shared_transaction.json"
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
fn common_consistency_patterns_compile_to_the_expected_plans() {
    for (name, document) in [
        ("eventual", EVENTUAL),
        ("causal_session", CAUSAL_SESSION),
        ("shared_transaction", SHARED_TRANSACTION),
    ] {
        BackendConfig::from_json(document)
            .unwrap_or_else(|error| panic!("{name} pattern must be valid: {error}"));
    }

    let default = PolicySelector::new(BackendConfig::from_json(DEFAULT).unwrap())
        .select("knowledge", &json!({ "channel_id": "channel-7" }))
        .unwrap();
    assert_eq!(default.consistency.snapshot_key, None);
    assert_eq!(
        default.consistency.allowed_sources,
        [ConsistencySource::Authority].into_iter().collect()
    );
    assert_eq!(
        default.consistency.linearization_key,
        Some(control_key([("channel_id", "channel-7")], []))
    );
    assert_eq!(
        default.consistency.commit_ordering_key,
        Some(control_key([("channel_id", "channel-7")], []))
    );
    assert_eq!(
        default.behavior.consistency.snapshot,
        SnapshotPolicy::Request
    );
    assert_eq!(default.behavior.conflict.strategy, ConflictStrategy::Reject);
    assert_eq!(default.behavior.publication, PublicationPolicy::Immediate);

    let eventual = PolicySelector::new(BackendConfig::from_json(EVENTUAL).unwrap())
        .select("knowledge", &json!({ "channel_id": "channel-7" }))
        .unwrap();
    assert_eq!(
        eventual.consistency.allowed_sources,
        [ConsistencySource::Authority, ConsistencySource::Replica]
            .into_iter()
            .collect()
    );
    assert!(eventual.consistency.causal.is_empty());
    assert_eq!(eventual.consistency.linearization_key, None);
    assert_eq!(eventual.consistency.commit_ordering_key, None);
    assert_eq!(
        eventual.behavior.conflict.strategy,
        ConflictStrategy::PreserveHeads
    );

    let causal = PolicySelector::new(BackendConfig::from_json(CAUSAL_SESSION).unwrap())
        .select(
            "knowledge",
            &json!({
                "channel_id": "channel-7",
                "plugin_route_id": "route-a"
            }),
        )
        .unwrap();
    assert_eq!(
        causal.consistency.causal,
        [CausalConsistencyPlan {
            field: "causal_token".to_owned(),
            minimum: None
        }]
    );
    assert_eq!(causal.consistency.sessions.len(), 1);
    assert_eq!(
        causal.consistency.sessions[0].key,
        control_key(
            [("channel_id", "channel-7")],
            [("participant_id", "route-a")]
        )
    );

    let shared = PolicySelector::new(BackendConfig::from_json(SHARED_TRANSACTION).unwrap())
        .select(
            "knowledge",
            &json!({
                "channel_id": "channel-7",
                "transaction_id": "transaction-3",
                "plugin_route_id": "route-a",
                "request_id": "request-9"
            }),
        )
        .unwrap();
    assert_eq!(
        shared.consistency.snapshot_key,
        Some(control_key(
            [("channel_id", "channel-7")],
            [("transaction_id", "transaction-3")]
        ))
    );
    assert_eq!(shared.publication_key, shared.consistency.snapshot_key);
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
    value["entity_types"]["knowledge"]["rules"][0]["behavior"]["consistency"]["snapshot"]["key_by"] =
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
        "causal_token": "causal-4",
        "base_versions": ["version-1"]
    });

    let selection = selector.select("knowledge", &meta).unwrap();
    assert_eq!(selection.rule.as_deref(), Some("transaction_batch"));
    assert_eq!(selection.scope, json_map([("channel_id", "channel-7")]));
    assert_eq!(
        selection.consistency.snapshot_key,
        Some(control_key(
            [("channel_id", "channel-7")],
            [("transaction_id", "transaction-3")]
        ))
    );
    assert_eq!(
        selection.consistency.allowed_sources,
        [ConsistencySource::Authority].into_iter().collect()
    );
    assert_eq!(
        selection.consistency.causal,
        [CausalConsistencyPlan {
            field: "causal_token".to_owned(),
            minimum: Some(json!("causal-4"))
        }]
    );
    assert_eq!(selection.consistency.linearization_key, None);
    assert_eq!(selection.consistency.sessions.len(), 1);
    assert_eq!(
        selection.consistency.sessions[0].key,
        control_key(
            [("channel_id", "channel-7")],
            [("participant_id", "route-a")]
        )
    );
    assert_eq!(
        selection.consistency.sessions[0].guarantees,
        [
            SessionGuarantee::MonotonicReads,
            SessionGuarantee::ReadYourWrites
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(
        selection.consistency.commit_ordering_key,
        Some(control_key([("channel_id", "channel-7")], []))
    );
    assert_eq!(
        selection.idempotency_key,
        Some(control_key(
            [("channel_id", "channel-7")],
            [
                ("participant_id", "route-a"),
                ("request_id", "request-9"),
                ("transaction_id", "transaction-3")
            ]
        ))
    );
    assert_eq!(
        selection.publication_key,
        selection.consistency.snapshot_key
    );
}

#[test]
fn request_consistency_has_no_shared_or_session_state() {
    let selector = PolicySelector::new(BackendConfig::from_json(EXAMPLE).unwrap());
    let meta = json!({ "channel_id": "channel-7" });

    let selection = selector.select("knowledge", &meta).unwrap();
    assert_eq!(selection.rule, None);
    assert_eq!(selection.scope, json_map([("channel_id", "channel-7")]));
    assert_eq!(selection.consistency.snapshot_key, None);
    assert!(selection.consistency.causal.is_empty());
    assert!(selection.consistency.sessions.is_empty());
    assert_eq!(
        selection.consistency.linearization_key,
        Some(control_key([("channel_id", "channel-7")], []))
    );
    assert_eq!(
        selection.consistency.commit_ordering_key,
        Some(control_key([("channel_id", "channel-7")], []))
    );
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
    assert_eq!(selection.consistency.snapshot_key, None);
    assert_eq!(
        selection.consistency.linearization_key,
        Some(control_key([], []))
    );
    assert_eq!(
        selection.consistency.commit_ordering_key,
        Some(control_key([], []))
    );
    assert_eq!(selection.idempotency_key, None);
    assert_eq!(selection.publication_key, None);
}

#[test]
fn configuration_rejects_incompatible_consistency_constraints() {
    let mut value: serde_json::Value = serde_json::from_str(EXAMPLE).unwrap();
    let behavior = &mut value["entity_types"]["knowledge"]["rules"][0]["behavior"];
    behavior["consistency"]["acquire"]["allow_sources"] = json!(["replica"]);
    behavior["consistency"]["acquire"]["requirements"] = json!([{
        "kind": "linearizable",
        "key_by": []
    }]);

    let error = BackendConfig::from_json(&value.to_string()).unwrap_err();
    assert!(matches!(error, ConfigError::Invalid { .. }));
    assert!(error.to_string().contains("requires authority"));
}

#[test]
fn configuration_requires_shared_snapshot_and_batch_keys_to_match() {
    let mut value: serde_json::Value = serde_json::from_str(EXAMPLE).unwrap();
    value["entity_types"]["knowledge"]["rules"][0]["behavior"]["publication"]["key_by"] =
        json!(["transaction_id", "participant_id"]);

    let error = BackendConfig::from_json(&value.to_string()).unwrap_err();
    assert!(matches!(error, ConfigError::Invalid { .. }));
    assert!(error.to_string().contains("must use the same key fields"));
}

#[test]
fn configuration_rejects_scope_fields_repeated_in_control_keys() {
    let mut value: serde_json::Value = serde_json::from_str(EXAMPLE).unwrap();
    value["entity_types"]["knowledge"]["fallback"]["consistency"]["acquire"]["requirements"][0]["key_by"] =
        json!(["channel_id"]);

    let error = BackendConfig::from_json(&value.to_string()).unwrap_err();
    assert!(matches!(error, ConfigError::Invalid { .. }));
    assert!(error.to_string().contains("implicit scope prefix"));
}

#[test]
fn configuration_rejects_duplicate_session_identities() {
    let mut value: serde_json::Value = serde_json::from_str(EXAMPLE).unwrap();
    value["entity_types"]["knowledge"]["rules"][0]["behavior"]["consistency"]["sessions"] = json!([
        {
            "key_by": ["participant_id"],
            "guarantees": ["monotonic_reads"]
        },
        {
            "key_by": ["participant_id"],
            "guarantees": ["read_your_writes"]
        }
    ]);

    let error = BackendConfig::from_json(&value.to_string()).unwrap_err();
    assert!(matches!(error, ConfigError::Invalid { .. }));
    assert!(error.to_string().contains("may be declared only once"));
}

#[test]
fn optional_causal_tokens_do_not_become_required_identity_fields() {
    let selector = PolicySelector::new(BackendConfig::from_json(EXAMPLE).unwrap());
    let selection = selector
        .select(
            "knowledge",
            &json!({
                "channel_id": "channel-7",
                "transaction_id": "transaction-3",
                "plugin_route_id": "route-a",
                "request_id": "request-9",
                "base_versions": ["version-1"]
            }),
        )
        .unwrap();

    assert_eq!(selection.rule.as_deref(), Some("transaction_batch"));
    assert_eq!(
        selection.consistency.causal,
        [CausalConsistencyPlan {
            field: "causal_token".to_owned(),
            minimum: None
        }]
    );
}

#[test]
fn linearizable_acquisition_intersects_sources_with_authority() {
    let mut value: serde_json::Value = serde_json::from_str(EXAMPLE).unwrap();
    value["entity_types"]["knowledge"]["fallback"]["consistency"]["acquire"]["allow_sources"] =
        json!(["authority", "replica"]);
    let selector =
        PolicySelector::new(BackendConfig::from_json(&value.to_string()).expect("valid config"));

    let selection = selector
        .select("knowledge", &json!({ "channel_id": "channel-7" }))
        .unwrap();

    assert_eq!(
        selection.consistency.allowed_sources,
        [ConsistencySource::Authority].into_iter().collect()
    );
}

#[test]
fn selector_validates_bound_metadata_values() {
    let selector = PolicySelector::new(BackendConfig::from_json(EXAMPLE).unwrap());
    let error = selector
        .select("knowledge", &json!({ "channel_id": "" }))
        .unwrap_err();

    assert!(error.to_string().contains("channel_id"));
    assert!(error.to_string().contains("invalid"));
}

fn json_map<const N: usize>(
    entries: [(&str, &str); N],
) -> std::collections::BTreeMap<String, serde_json::Value> {
    entries
        .into_iter()
        .map(|(name, value)| (name.to_owned(), json!(value)))
        .collect()
}

fn control_key<const S: usize, const F: usize>(
    scope: [(&str, &str); S],
    fields: [(&str, &str); F],
) -> ControlKey {
    ControlKey {
        scope: json_map(scope),
        fields: json_map(fields),
    }
}
