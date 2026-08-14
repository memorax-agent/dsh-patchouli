use std::collections::BTreeMap;

use patchouli_backend::{
    BackendError, BackendErrorReason, CreateEntityData, CreateEntityParams, EntityRef,
    EntityVersion, JsonRpcNotification, JsonRpcVersion, PROTOCOL_VERSION, error_codes, methods,
};
use serde_json::{Value, json};

#[test]
fn protocol_identity_matches_the_client_contract() {
    assert_eq!(PROTOCOL_VERSION, 1);
    assert_eq!(methods::ENTITY_CREATE, "patchouli.entity.create@1");
    assert_eq!(methods::CHANGES_EVENT, "patchouli.changes.event@1");
}

#[test]
fn rust_method_and_error_constants_match_openrpc() {
    let document: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/protocol/openrpc.json"
    )))
    .unwrap();

    let mut documented_methods = document["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|method| method["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    documented_methods.sort_unstable();

    let mut rust_methods = vec![
        methods::HANDSHAKE,
        methods::CONTROL_STATUS,
        methods::CONTROL_CHECKPOINT,
        methods::CONTROL_SHUTDOWN,
        methods::ENTITY_CREATE,
        methods::ENTITY_READ,
        methods::ENTITY_RETRIEVE,
        methods::ENTITY_UPDATE,
        methods::ENTITY_DELETE,
        methods::CHANGES_SUBSCRIBE,
        methods::CHANGES_UNSUBSCRIBE,
        methods::CHANGES_EVENT,
    ];
    rust_methods.sort_unstable();
    assert_eq!(rust_methods, documented_methods);

    let errors = document["components"]["errors"].as_object().unwrap();
    assert_eq!(
        errors["InvalidRequest"]["code"],
        error_codes::INVALID_REQUEST
    );
    assert_eq!(
        errors["Unauthenticated"]["code"],
        error_codes::UNAUTHENTICATED
    );
    assert_eq!(errors["Forbidden"]["code"], error_codes::FORBIDDEN);
    assert_eq!(errors["NotFound"]["code"], error_codes::NOT_FOUND);
    assert_eq!(
        errors["VersionConflict"]["code"],
        error_codes::VERSION_CONFLICT
    );
    assert_eq!(
        errors["IdempotencyConflict"]["code"],
        error_codes::IDEMPOTENCY_CONFLICT
    );
    assert_eq!(
        errors["UnsupportedCapability"]["code"],
        error_codes::UNSUPPORTED_CAPABILITY
    );
    assert_eq!(
        errors["DeadlineExceeded"]["code"],
        error_codes::DEADLINE_EXCEEDED
    );
    assert_eq!(errors["Overloaded"]["code"], error_codes::OVERLOADED);
    assert_eq!(errors["CursorExpired"]["code"], error_codes::CURSOR_EXPIRED);
    assert_eq!(
        errors["WorkUnitExpired"]["code"],
        error_codes::WORK_UNIT_EXPIRED
    );
}

#[test]
fn entity_version_uses_the_json_rpc_field_names() {
    let entity = EntityVersion::Active {
        entity_ref: EntityRef {
            entity_type: "knowledge".to_owned(),
            id: "entity-1".to_owned(),
        },
        version: "version-1".to_owned(),
        value: json!({ "text": "hello" }),
    };

    assert_eq!(
        serde_json::to_value(entity).unwrap(),
        json!({
            "state": "active",
            "ref": { "type": "knowledge", "id": "entity-1" },
            "version": "version-1",
            "value": { "text": "hello" }
        })
    );
}

#[test]
fn business_params_have_only_meta_and_data_at_the_top_level() {
    let params = CreateEntityParams {
        meta: BTreeMap::from([
            ("channel_id".to_owned(), json!("channel-7")),
            ("causal_token".to_owned(), json!("causal-2")),
        ]),
        data: CreateEntityData {
            entity_type: "knowledge".to_owned(),
            id: Some("entity-1".to_owned()),
            value: json!({ "text": "hello" }),
        },
    };

    assert_eq!(
        serde_json::to_value(params).unwrap(),
        json!({
            "meta": {
                "channel_id": "channel-7",
                "causal_token": "causal-2"
            },
            "data": {
                "type": "knowledge",
                "id": "entity-1",
                "value": { "text": "hello" }
            }
        })
    );
}

#[test]
fn version_conflicts_preserve_the_current_heads() {
    let error = BackendError::version_conflict(vec!["head-a".to_owned(), "head-b".to_owned()]);

    assert_eq!(error.reason, BackendErrorReason::VersionConflict);
    assert_eq!(error.current_versions, ["head-a", "head-b"]);
}

#[test]
fn change_events_are_json_rpc_notifications() {
    let event = JsonRpcNotification {
        jsonrpc: JsonRpcVersion::V2,
        method: methods::CHANGES_EVENT.to_owned(),
        params: json!({ "subscription_id": "sub-1" }),
    };

    let value = serde_json::to_value(event).unwrap();
    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["method"], methods::CHANGES_EVENT);
    assert!(value.get("id").is_none());
}
