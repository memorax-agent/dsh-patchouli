use std::{collections::BTreeMap, sync::Arc, time::Duration};

use futures_util::StreamExt;
use patchouli_backend::{
    BackendConfig, BackendEngine, BackendErrorReason, BackendService, CreateEntityData,
    DeleteEntityData, EntityRef, EntityVersion, ReadEntityData, ReadState, RetrieveEntitiesData,
    RpcParams, SubscribeChangesData, UpdateEntityData,
};
use patchouli_provider_sqlite::SqliteProvider;
use serde_json::{Value, json};
use tokio_rusqlite::rusqlite::Connection;

const DEFAULT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../config/patchouli.default.json"
));
const CAUSAL_SESSION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../config/patterns/causal_session.json"
));
const KNOWLEDGE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packages/protocol/schemas/examples/knowledge@1.json"
));
const KNOWLEDGE_RELATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packages/protocol/schemas/examples/knowledge-relation@1.json"
));

#[tokio::test]
async fn sqlite_replays_idempotent_mutations_and_retrieves_and_streams_changes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("patchouli.db");
    let mut config: Value = serde_json::from_str(DEFAULT).unwrap();
    config["meta_fields"]["request_id"] = json!({
        "pointer": "/request_id",
        "schema": { "type": "string", "minLength": 1 }
    });
    config["entity_types"]["knowledge"]["fallback"]["idempotency"] =
        json!({ "mode": "keyed", "key_by": ["request_id"] });
    let engine = start_engine_with(&path, &config.to_string()).await;
    let mut subscription = engine
        .subscribe(RpcParams {
            meta: meta([]),
            data: SubscribeChangesData {
                filter: None,
                after_cursor: None,
            },
        })
        .await
        .unwrap();
    let params = RpcParams {
        meta: meta([("request_id", json!("request-1"))]),
        data: CreateEntityData {
            entity_type: "knowledge".to_owned(),
            id: Some("knowledge-idempotent".to_owned()),
            value: knowledge("needle context", "idempotent"),
        },
    };
    let first = engine.create(params.clone()).await.unwrap();
    let replay = engine.create(params).await.unwrap();
    assert_eq!(first, replay);
    let event = subscription.stream.next().await.unwrap().unwrap();
    assert_eq!(event.change.entity_ref.id, "knowledge-idempotent");
    assert_eq!(event.meta.get("request_id"), Some(&json!("request-1")));

    let found = engine
        .retrieve(RpcParams {
            meta: meta([]),
            data: RetrieveEntitiesData {
                query: json!({ "text": "needle" }).to_string(),
                types: Some(vec!["knowledge".to_owned()]),
                limit: 10,
            },
        })
        .await
        .unwrap();
    assert_eq!(found.data.hits.len(), 1);

    let conflict = engine
        .create(RpcParams {
            meta: meta([("request_id", json!("request-1"))]),
            data: CreateEntityData {
                entity_type: "knowledge".to_owned(),
                id: Some("knowledge-other".to_owned()),
                value: knowledge("different", "idempotent"),
            },
        })
        .await
        .unwrap_err();
    assert_eq!(conflict.reason, BackendErrorReason::IdempotencyConflict);
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn idempotency_replay_still_enforces_the_selected_consistency_frontier() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("patchouli.db");
    let mut config: Value = serde_json::from_str(CAUSAL_SESSION).unwrap();
    config["meta_fields"]["request_id"] = json!({
        "pointer": "/request_id",
        "schema": { "type": "string", "minLength": 1 }
    });
    config["entity_types"]["knowledge"]["fallback"]["idempotency"] =
        json!({ "mode": "keyed", "key_by": ["request_id"] });
    let engine = start_engine_with(&path, &config.to_string()).await;
    let data = CreateEntityData {
        entity_type: "knowledge".to_owned(),
        id: Some("consistent-replay".to_owned()),
        value: knowledge("consistent replay", "idempotency"),
    };
    let request_meta = meta([
        ("plugin_route_id", json!("participant-1")),
        ("request_id", json!("consistent-request")),
    ]);
    let first = engine
        .create(RpcParams {
            meta: request_meta.clone(),
            data: data.clone(),
        })
        .await
        .unwrap();
    let unavailable = engine
        .create(RpcParams {
            meta: {
                let mut value = request_meta.clone();
                value.insert("causal_token".to_owned(), json!("unknown-frontier"));
                value
            },
            data: data.clone(),
        })
        .await
        .unwrap_err();
    assert_eq!(unavailable.reason, BackendErrorReason::Overloaded);
    let replay = engine
        .create(RpcParams {
            meta: request_meta,
            data,
        })
        .await
        .unwrap();
    assert_eq!(replay, first);
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn default_retrieval_across_all_entity_types_does_not_reacquire_the_same_lock() {
    let directory = tempfile::tempdir().unwrap();
    let engine = start_engine(&directory.path().join("patchouli.db")).await;
    let result = tokio::time::timeout(
        Duration::from_secs(1),
        engine.retrieve(RpcParams {
            meta: meta([]),
            data: RetrieveEntitiesData {
                query: json!({ "text": "nothing" }).to_string(),
                types: None,
                limit: 10,
            },
        }),
    )
    .await
    .expect("multi-type retrieval must not deadlock")
    .unwrap();
    assert!(result.data.hits.is_empty());
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn json_retrieval_queries_filter_relations_and_page_stably() {
    let directory = tempfile::tempdir().unwrap();
    let engine = start_engine(&directory.path().join("patchouli.db")).await;
    for (id, text, source, sequence) in [
        ("knowledge-a", "alpha needle context", "first", 1),
        ("knowledge-b", "beta context", "second", 2),
        ("knowledge-c", "gamma context", "first", 3),
    ] {
        let mut value = knowledge(text, source);
        value["metadata"]["extensions"]["test.source"]["sequence"] = json!(sequence);
        engine
            .create(RpcParams {
                meta: meta([]),
                data: CreateEntityData {
                    entity_type: "knowledge".to_owned(),
                    id: Some(id.to_owned()),
                    value,
                },
            })
            .await
            .unwrap();
    }

    let mut relation: Value = serde_json::from_str(KNOWLEDGE_RELATION).unwrap();
    relation["from"] = json!([{ "type": "knowledge", "id": "knowledge-a" }]);
    relation["to"] = json!([{ "type": "knowledge", "id": "knowledge-b" }]);
    engine
        .create(RpcParams {
            meta: meta([]),
            data: CreateEntityData {
                entity_type: "knowledge_relation".to_owned(),
                id: Some("relation-a-b".to_owned()),
                value: relation,
            },
        })
        .await
        .unwrap();

    let filtered = engine
        .retrieve(RpcParams {
            meta: meta([]),
            data: RetrieveEntitiesData {
                query: json!({
                    "ids": ["knowledge-a", "knowledge-c"],
                    "where": {
                        "/metadata/extensions/test.source/source": "first",
                        "/metadata/extensions/test.source/sequence": { "$gte": 2 }
                    },
                    "order": "id_asc"
                })
                .to_string(),
                types: Some(vec!["knowledge".to_owned()]),
                limit: 10,
            },
        })
        .await
        .unwrap();
    assert_eq!(hit_ids(&filtered.data.hits), ["knowledge-c"]);

    let text = engine
        .retrieve(RpcParams {
            meta: meta([]),
            data: RetrieveEntitiesData {
                query: json!({ "text": "alpha needle" }).to_string(),
                types: Some(vec!["knowledge".to_owned()]),
                limit: 10,
            },
        })
        .await
        .unwrap();
    assert_eq!(hit_ids(&text.data.hits), ["knowledge-a"]);
    assert!(text.data.hits[0].score > 0.0);

    let related = engine
        .retrieve(RpcParams {
            meta: meta([]),
            data: RetrieveEntitiesData {
                query: json!({
                    "where": {
                        "/from": {
                            "$contains": { "type": "knowledge", "id": "knowledge-a" }
                        }
                    }
                })
                .to_string(),
                types: Some(vec!["knowledge_relation".to_owned()]),
                limit: 10,
            },
        })
        .await
        .unwrap();
    assert_eq!(hit_ids(&related.data.hits), ["relation-a-b"]);

    let first_page = engine
        .retrieve(RpcParams {
            meta: meta([]),
            data: RetrieveEntitiesData {
                query: json!({ "order": "id_asc" }).to_string(),
                types: Some(vec!["knowledge".to_owned()]),
                limit: 2,
            },
        })
        .await
        .unwrap();
    assert_eq!(
        hit_ids(&first_page.data.hits),
        ["knowledge-a", "knowledge-b"]
    );
    let cursor = first_page.meta.get("next_cursor").unwrap().clone();
    let second_page = engine
        .retrieve(RpcParams {
            meta: meta([]),
            data: RetrieveEntitiesData {
                query: json!({ "order": "id_asc", "cursor": cursor.clone() }).to_string(),
                types: Some(vec!["knowledge".to_owned()]),
                limit: 2,
            },
        })
        .await
        .unwrap();
    assert_eq!(hit_ids(&second_page.data.hits), ["knowledge-c"]);
    assert!(!second_page.meta.contains_key("next_cursor"));

    let mismatched_cursor = engine
        .retrieve(RpcParams {
            meta: meta([]),
            data: RetrieveEntitiesData {
                query: json!({
                    "ids": ["knowledge-a"],
                    "order": "id_asc",
                    "cursor": cursor,
                })
                .to_string(),
                types: Some(vec!["knowledge".to_owned()]),
                limit: 2,
            },
        })
        .await
        .unwrap_err();
    assert_eq!(mismatched_cursor.reason, BackendErrorReason::InvalidRequest);
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn expired_deadline_rejects_mutation_before_acceptance() {
    let directory = tempfile::tempdir().unwrap();
    let engine = start_engine(&directory.path().join("patchouli.db")).await;
    let error = engine
        .create(RpcParams {
            meta: meta([("deadline_unix_ms", json!(0))]),
            data: CreateEntityData {
                entity_type: "knowledge".to_owned(),
                id: Some("expired-deadline".to_owned()),
                value: knowledge("must not persist", "deadline"),
            },
        })
        .await
        .unwrap_err();
    assert_eq!(error.reason, BackendErrorReason::DeadlineExceeded);

    let read = engine
        .read(RpcParams {
            meta: meta([]),
            data: ReadEntityData {
                entity_ref: EntityRef {
                    entity_type: "knowledge".to_owned(),
                    id: "expired-deadline".to_owned(),
                },
            },
        })
        .await
        .unwrap_err();
    assert_eq!(read.reason, BackendErrorReason::NotFound);
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn causal_and_session_frontiers_survive_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("patchouli.db");
    let mut request_meta = meta([("plugin_route_id", json!("plugin-1"))]);
    let engine = start_engine_with(&path, CAUSAL_SESSION).await;
    let created = engine
        .create(RpcParams {
            meta: request_meta.clone(),
            data: CreateEntityData {
                entity_type: "knowledge".to_owned(),
                id: Some("causal-knowledge".to_owned()),
                value: knowledge("causal content", "session"),
            },
        })
        .await
        .unwrap();
    let token = created
        .meta
        .get("causal_token")
        .and_then(Value::as_str)
        .unwrap()
        .to_owned();
    request_meta.insert("causal_token".to_owned(), json!(token));
    engine
        .read(RpcParams {
            meta: request_meta.clone(),
            data: ReadEntityData {
                entity_ref: EntityRef {
                    entity_type: "knowledge".to_owned(),
                    id: "causal-knowledge".to_owned(),
                },
            },
        })
        .await
        .unwrap();
    engine.shutdown().await.unwrap();

    let restarted = start_engine_with(&path, CAUSAL_SESSION).await;
    let read = restarted
        .read(RpcParams {
            meta: request_meta,
            data: ReadEntityData {
                entity_ref: EntityRef {
                    entity_type: "knowledge".to_owned(),
                    id: "causal-knowledge".to_owned(),
                },
            },
        })
        .await
        .unwrap();
    assert_eq!(read.meta.get("causal_token"), Some(&json!(token)));
    let unavailable = restarted
        .read(RpcParams {
            meta: meta([
                ("plugin_route_id", json!("plugin-1")),
                ("causal_token", json!("unknown-frontier")),
            ]),
            data: ReadEntityData {
                entity_ref: EntityRef {
                    entity_type: "knowledge".to_owned(),
                    id: "causal-knowledge".to_owned(),
                },
            },
        })
        .await
        .unwrap_err();
    assert_eq!(unavailable.reason, BackendErrorReason::Overloaded);
    restarted.shutdown().await.unwrap();
}

#[tokio::test]
async fn batch_acceptance_and_idempotency_commit_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("patchouli.db");
    let mut config: Value = serde_json::from_str(DEFAULT).unwrap();
    config["meta_fields"]["request_id"] = json!({
        "pointer": "/request_id",
        "schema": { "type": "string", "minLength": 1 }
    });
    for rule in config["entity_types"]["knowledge"]["rules"]
        .as_array_mut()
        .unwrap()
    {
        rule["behavior"]["idempotency"] = json!({ "mode": "keyed", "key_by": ["request_id"] });
    }
    let engine = start_engine_with(&path, &config.to_string()).await;
    let mut subscription = engine
        .subscribe(RpcParams {
            meta: meta([]),
            data: SubscribeChangesData {
                filter: None,
                after_cursor: None,
            },
        })
        .await
        .unwrap();
    let first = RpcParams {
        meta: work_unit_meta(
            "idempotent-batch",
            false,
            [("request_id", json!("batch-request-1"))],
        ),
        data: CreateEntityData {
            entity_type: "knowledge".to_owned(),
            id: Some("batch-idempotent-a".to_owned()),
            value: knowledge("first", "batch"),
        },
    };
    let accepted = engine.create(first.clone()).await.unwrap();
    assert_eq!(engine.create(first).await.unwrap(), accepted);
    engine
        .create(RpcParams {
            meta: work_unit_meta(
                "idempotent-batch",
                true,
                [("request_id", json!("batch-request-2"))],
            ),
            data: CreateEntityData {
                entity_type: "knowledge".to_owned(),
                id: Some("batch-idempotent-b".to_owned()),
                value: knowledge("second", "batch"),
            },
        })
        .await
        .unwrap();
    let first_event = tokio::time::timeout(Duration::from_secs(1), subscription.stream.next())
        .await
        .expect("batch publication must wake subscriptions")
        .unwrap()
        .unwrap();
    let second_event = tokio::time::timeout(Duration::from_secs(1), subscription.stream.next())
        .await
        .expect("batch publication must publish every member")
        .unwrap()
        .unwrap();
    let ids = [
        first_event.change.entity_ref.id,
        second_event.change.entity_ref.id,
    ];
    assert!(ids.contains(&"batch-idempotent-a".to_owned()));
    assert!(ids.contains(&"batch-idempotent-b".to_owned()));
    let visible = engine
        .read(RpcParams {
            meta: meta([]),
            data: ReadEntityData {
                entity_ref: EntityRef {
                    entity_type: "knowledge".to_owned(),
                    id: "batch-idempotent-a".to_owned(),
                },
            },
        })
        .await
        .unwrap();
    assert_eq!(visible.data.state, ReadState::Active);
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn expired_batch_discards_staged_idempotency_instead_of_replaying_success() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("patchouli.db");
    let mut config: Value = serde_json::from_str(DEFAULT).unwrap();
    config["meta_fields"]["request_id"] = json!({
        "pointer": "/request_id",
        "schema": { "type": "string", "minLength": 1 }
    });
    for rule in config["entity_types"]["knowledge"]["rules"]
        .as_array_mut()
        .unwrap()
    {
        rule["behavior"]["idempotency"] = json!({ "mode": "keyed", "key_by": ["request_id"] });
        rule["behavior"]["publication"]["staging_ttl_ms"] = json!(50);
    }
    let engine = start_engine_with(&path, &config.to_string()).await;
    let request = RpcParams {
        meta: work_unit_meta(
            "expiring-idempotent-batch",
            false,
            [("request_id", json!("expiring-request"))],
        ),
        data: CreateEntityData {
            entity_type: "knowledge".to_owned(),
            id: Some("expiring-batch-entity".to_owned()),
            value: knowledge("staged", "batch"),
        },
    };
    engine.create(request.clone()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;
    let error = engine.create(request).await.unwrap_err();
    assert_eq!(error.reason, BackendErrorReason::WorkUnitExpired);
    engine.shutdown().await.unwrap();

    let connection = Connection::open(path).unwrap();
    let staged: u64 = connection
        .query_row(
            "SELECT count(*) FROM patchouli_work_unit_idempotency",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(staged, 0);
}

#[tokio::test]
async fn default_sqlite_crud_is_durable_and_transactional() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("patchouli.db");
    let engine = start_engine(&path).await;
    let entity_ref = EntityRef {
        entity_type: "knowledge".to_owned(),
        id: "knowledge-1".to_owned(),
    };

    let created = engine
        .create(RpcParams {
            meta: meta([]),
            data: CreateEntityData {
                entity_type: entity_ref.entity_type.clone(),
                id: Some(entity_ref.id.clone()),
                value: knowledge("some value", "base"),
            },
        })
        .await
        .unwrap();
    let created_version = version(&created.data.entity);

    let read = engine
        .read(RpcParams {
            meta: meta([]),
            data: ReadEntityData {
                entity_ref: entity_ref.clone(),
            },
        })
        .await
        .unwrap();
    assert_eq!(read.data.state, ReadState::Active);
    assert_eq!(read.data.variants.len(), 1);

    let updated = engine
        .update(RpcParams {
            meta: meta([("base_versions", json!([created_version]))]),
            data: UpdateEntityData {
                entity_ref: entity_ref.clone(),
                value: knowledge("updated", "update"),
            },
        })
        .await
        .unwrap();
    let updated_version = version(&updated.data.entity);

    let deleted = engine
        .delete(RpcParams {
            meta: meta([("base_versions", json!([updated_version]))]),
            data: DeleteEntityData {
                entity_ref: entity_ref.clone(),
            },
        })
        .await
        .unwrap();
    assert!(matches!(deleted.data.entity, EntityVersion::Deleted { .. }));

    let read = engine
        .read(RpcParams {
            meta: meta([]),
            data: ReadEntityData { entity_ref },
        })
        .await
        .unwrap();
    assert_eq!(read.data.state, ReadState::Deleted);

    engine.shutdown().await.unwrap();
    let connection = Connection::open(path).unwrap();
    let counts: (u32, u32, u32, u32) = connection
        .query_row(
            "SELECT
                (SELECT count(*) FROM patchouli_entity_version),
                (SELECT count(*) FROM patchouli_change),
                (SELECT count(*) FROM patchouli_entity_head),
                (SELECT count(*) FROM patchouli_entity_version
                 WHERE published_cursor IS NULL)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(counts, (3, 3, 1, 0));
}

#[tokio::test]
async fn stale_knowledge_updates_merge_content_and_keep_mvcc_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let engine = start_engine(&directory.path().join("patchouli.db")).await;
    let entity_ref = EntityRef {
        entity_type: "knowledge".to_owned(),
        id: "knowledge-conflict".to_owned(),
    };
    let created = engine
        .create(RpcParams {
            meta: meta([]),
            data: CreateEntityData {
                entity_type: entity_ref.entity_type.clone(),
                id: Some(entity_ref.id.clone()),
                value: knowledge("some value", "base"),
            },
        })
        .await
        .unwrap();
    let base = version(&created.data.entity);

    engine
        .update(RpcParams {
            meta: meta([("base_versions", json!([base]))]),
            data: UpdateEntityData {
                entity_ref: entity_ref.clone(),
                value: knowledge("some day", "first"),
            },
        })
        .await
        .unwrap();
    engine
        .update(RpcParams {
            meta: meta([("base_versions", json!([base]))]),
            data: UpdateEntityData {
                entity_ref: entity_ref.clone(),
                value: knowledge("another value", "second"),
            },
        })
        .await
        .unwrap();

    let conflicted = engine
        .read(RpcParams {
            meta: meta([]),
            data: ReadEntityData {
                entity_ref: entity_ref.clone(),
            },
        })
        .await
        .unwrap();
    assert_eq!(conflicted.data.state, ReadState::Conflicted);
    assert_eq!(conflicted.data.variants.len(), 2);
    assert!(
        conflicted
            .data
            .variants
            .iter()
            .all(|variant| match variant {
                EntityVersion::Active { value, .. } => {
                    value.pointer("/content/text") == Some(&json!("another day"))
                }
                EntityVersion::Deleted { .. } => false,
            })
    );
    let heads = conflicted
        .data
        .variants
        .iter()
        .map(version)
        .collect::<Vec<_>>();
    let resolved_value = match &conflicted.data.variants[0] {
        EntityVersion::Active { value, .. } => value.clone(),
        EntityVersion::Deleted { .. } => unreachable!(),
    };
    engine
        .update(RpcParams {
            meta: meta([("base_versions", json!(heads))]),
            data: UpdateEntityData {
                entity_ref: entity_ref.clone(),
                value: resolved_value,
            },
        })
        .await
        .unwrap();
    let resolved = engine
        .read(RpcParams {
            meta: meta([]),
            data: ReadEntityData { entity_ref },
        })
        .await
        .unwrap();
    assert_eq!(resolved.data.state, ReadState::Active);
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn request_reject_strategy_reports_current_heads_for_a_stale_update() {
    let directory = tempfile::tempdir().unwrap();
    let engine = start_engine(&directory.path().join("patchouli.db")).await;
    let entity_ref = EntityRef {
        entity_type: "knowledge".to_owned(),
        id: "knowledge-reject".to_owned(),
    };
    let created = engine
        .create(RpcParams {
            meta: meta([]),
            data: CreateEntityData {
                entity_type: entity_ref.entity_type.clone(),
                id: Some(entity_ref.id.clone()),
                value: knowledge("base", "base"),
            },
        })
        .await
        .unwrap();
    let base = version(&created.data.entity);
    engine
        .update(RpcParams {
            meta: meta([("base_versions", json!([base]))]),
            data: UpdateEntityData {
                entity_ref: entity_ref.clone(),
                value: knowledge("first", "first"),
            },
        })
        .await
        .unwrap();

    let error = engine
        .update(RpcParams {
            meta: meta([
                ("base_versions", json!([base])),
                ("conflict_strategy", json!("reject")),
            ]),
            data: UpdateEntityData {
                entity_ref,
                value: knowledge("second", "second"),
            },
        })
        .await
        .unwrap_err();
    assert_eq!(error.reason, BackendErrorReason::VersionConflict);
    assert_eq!(error.current_versions.len(), 1);
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn request_reject_strategy_requires_the_exact_current_head_set() {
    let directory = tempfile::tempdir().unwrap();
    let engine = start_engine(&directory.path().join("patchouli.db")).await;
    let entity_ref = EntityRef {
        entity_type: "knowledge".to_owned(),
        id: "knowledge-reject-exact".to_owned(),
    };
    let created = engine
        .create(RpcParams {
            meta: meta([]),
            data: CreateEntityData {
                entity_type: entity_ref.entity_type.clone(),
                id: Some(entity_ref.id.clone()),
                value: knowledge("base", "base"),
            },
        })
        .await
        .unwrap();
    let old = version(&created.data.entity);
    let current = engine
        .update(RpcParams {
            meta: meta([("base_versions", json!([old]))]),
            data: UpdateEntityData {
                entity_ref: entity_ref.clone(),
                value: knowledge("current", "current"),
            },
        })
        .await
        .unwrap();
    let current = version(&current.data.entity);
    let non_exact = json!([old, current]);

    let update = engine
        .update(RpcParams {
            meta: meta([
                ("base_versions", non_exact.clone()),
                ("conflict_strategy", json!("reject")),
            ]),
            data: UpdateEntityData {
                entity_ref: entity_ref.clone(),
                value: knowledge("must reject", "update"),
            },
        })
        .await
        .unwrap_err();
    assert_eq!(update.reason, BackendErrorReason::VersionConflict);

    let delete = engine
        .delete(RpcParams {
            meta: meta([
                ("base_versions", non_exact),
                ("conflict_strategy", json!("reject")),
            ]),
            data: DeleteEntityData { entity_ref },
        })
        .await
        .unwrap_err();
    assert_eq!(delete.reason, BackendErrorReason::VersionConflict);
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn work_unit_survives_restart_and_publishes_all_entities_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("patchouli.db");
    let first = start_engine_with(&path, DEFAULT).await;
    let first_ref = EntityRef {
        entity_type: "knowledge".to_owned(),
        id: "staged-first".to_owned(),
    };
    let second_ref = EntityRef {
        entity_type: "knowledge".to_owned(),
        id: "staged-second".to_owned(),
    };

    first
        .create(RpcParams {
            meta: work_unit_meta("work-1", false, []),
            data: CreateEntityData {
                entity_type: first_ref.entity_type.clone(),
                id: Some(first_ref.id.clone()),
                value: knowledge("first", "staged"),
            },
        })
        .await
        .unwrap();
    assert_eq!(
        first
            .read(RpcParams {
                meta: meta([]),
                data: ReadEntityData {
                    entity_ref: first_ref.clone(),
                },
            })
            .await
            .unwrap_err()
            .reason,
        BackendErrorReason::NotFound
    );
    assert_eq!(
        first
            .read(RpcParams {
                meta: work_unit_meta("work-1", false, []),
                data: ReadEntityData {
                    entity_ref: first_ref.clone(),
                },
            })
            .await
            .unwrap()
            .data
            .state,
        ReadState::Active
    );
    first.shutdown().await.unwrap();

    let second = start_engine_with(&path, DEFAULT).await;
    assert_eq!(
        second
            .read(RpcParams {
                meta: work_unit_meta("work-1", false, []),
                data: ReadEntityData {
                    entity_ref: first_ref.clone(),
                },
            })
            .await
            .unwrap()
            .data
            .state,
        ReadState::Active
    );
    second
        .create(RpcParams {
            meta: work_unit_meta("work-1", true, []),
            data: CreateEntityData {
                entity_type: second_ref.entity_type.clone(),
                id: Some(second_ref.id.clone()),
                value: knowledge("second", "commit"),
            },
        })
        .await
        .unwrap();

    for entity_ref in [first_ref, second_ref] {
        assert_eq!(
            second
                .read(RpcParams {
                    meta: meta([]),
                    data: ReadEntityData { entity_ref },
                })
                .await
                .unwrap()
                .data
                .state,
            ReadState::Active
        );
    }
    assert_eq!(
        second
            .read(RpcParams {
                meta: work_unit_meta("work-1", false, []),
                data: ReadEntityData {
                    entity_ref: EntityRef {
                        entity_type: "knowledge".to_owned(),
                        id: "staged-first".to_owned(),
                    },
                },
            })
            .await
            .unwrap_err()
            .reason,
        BackendErrorReason::InvalidRequest
    );
    second.shutdown().await.unwrap();

    let connection = Connection::open(path).unwrap();
    let counts: (u32, u32, u32, u32) = connection
        .query_row(
            "SELECT
                (SELECT count(*) FROM patchouli_work_unit WHERE state = 'committed'),
                (SELECT count(*) FROM patchouli_entity_head),
                (SELECT count(*) FROM patchouli_change),
                (SELECT count(*) FROM patchouli_entity_version
                 WHERE published_cursor IS NULL)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(counts, (1, 2, 2, 0));
}

#[tokio::test]
async fn work_unit_resolves_external_baseline_drift_with_configured_merge_and_mvcc() {
    let directory = tempfile::tempdir().unwrap();
    let engine = start_engine_with(&directory.path().join("patchouli.db"), DEFAULT).await;
    let entity_ref = EntityRef {
        entity_type: "knowledge".to_owned(),
        id: "publish-race".to_owned(),
    };
    let created = engine
        .create(RpcParams {
            meta: meta([]),
            data: CreateEntityData {
                entity_type: entity_ref.entity_type.clone(),
                id: Some(entity_ref.id.clone()),
                value: knowledge("base", "base"),
            },
        })
        .await
        .unwrap();
    let base = version(&created.data.entity);

    engine
        .read(RpcParams {
            meta: work_unit_meta("work-race", false, []),
            data: ReadEntityData {
                entity_ref: entity_ref.clone(),
            },
        })
        .await
        .unwrap();
    let outside = engine
        .update(RpcParams {
            meta: meta([("base_versions", json!([base]))]),
            data: UpdateEntityData {
                entity_ref: entity_ref.clone(),
                value: knowledge("outside", "outside"),
            },
        })
        .await
        .unwrap();
    let outside_version = version(&outside.data.entity);

    let resolved = engine
        .update(RpcParams {
            meta: work_unit_meta("work-race", true, [("base_versions", json!([base]))]),
            data: UpdateEntityData {
                entity_ref: entity_ref.clone(),
                value: knowledge("inside", "inside"),
            },
        })
        .await
        .unwrap();
    let resolved_version = version(&resolved.data.entity);

    let visible = engine
        .read(RpcParams {
            meta: meta([]),
            data: ReadEntityData { entity_ref },
        })
        .await
        .unwrap();
    assert_eq!(visible.data.state, ReadState::Conflicted);
    assert_eq!(visible.data.variants.len(), 2);
    assert!(
        visible
            .data
            .variants
            .iter()
            .all(|variant| version(variant) != outside_version)
    );
    assert!(
        visible
            .data
            .variants
            .iter()
            .any(|variant| version(variant) == resolved_version)
    );
    let merged_content = visible
        .data
        .variants
        .iter()
        .map(|variant| match variant {
            EntityVersion::Active { value, .. } => value.pointer("/content/text").unwrap(),
            EntityVersion::Deleted { .. } => unreachable!(),
        })
        .collect::<Vec<_>>();
    assert_eq!(merged_content[0], merged_content[1]);
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn work_unit_rejects_external_baseline_drift_when_requested() {
    let directory = tempfile::tempdir().unwrap();
    let engine = start_engine_with(&directory.path().join("patchouli.db"), DEFAULT).await;
    let entity_ref = EntityRef {
        entity_type: "knowledge".to_owned(),
        id: "publish-reject".to_owned(),
    };
    let created = engine
        .create(RpcParams {
            meta: meta([]),
            data: CreateEntityData {
                entity_type: entity_ref.entity_type.clone(),
                id: Some(entity_ref.id.clone()),
                value: knowledge("base", "base"),
            },
        })
        .await
        .unwrap();
    let base = version(&created.data.entity);
    engine
        .read(RpcParams {
            meta: work_unit_meta("work-reject", false, []),
            data: ReadEntityData {
                entity_ref: entity_ref.clone(),
            },
        })
        .await
        .unwrap();
    let outside = engine
        .update(RpcParams {
            meta: meta([("base_versions", json!([base]))]),
            data: UpdateEntityData {
                entity_ref: entity_ref.clone(),
                value: knowledge("outside", "outside"),
            },
        })
        .await
        .unwrap();
    let error = engine
        .update(RpcParams {
            meta: work_unit_meta(
                "work-reject",
                true,
                [
                    ("base_versions", json!([base])),
                    ("conflict_strategy", json!("reject")),
                ],
            ),
            data: UpdateEntityData {
                entity_ref: entity_ref.clone(),
                value: knowledge("inside", "inside"),
            },
        })
        .await
        .unwrap_err();
    assert_eq!(error.reason, BackendErrorReason::VersionConflict);
    assert!(error.current_versions.is_empty());
    assert_eq!(error.conflicts.len(), 1);
    assert_eq!(error.conflicts[0].entity_ref.id, "publish-reject");
    assert_eq!(
        error.conflicts[0].current_versions,
        vec![version(&outside.data.entity)]
    );
    let sealed = engine
        .create(RpcParams {
            meta: work_unit_meta("work-reject", false, []),
            data: CreateEntityData {
                entity_type: "knowledge".to_owned(),
                id: Some("must-not-join-after-close".to_owned()),
                value: knowledge("late", "late"),
            },
        })
        .await
        .unwrap_err();
    assert_eq!(sealed.reason, BackendErrorReason::InvalidRequest);
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn work_unit_reports_every_rejected_entity_conflict() {
    let directory = tempfile::tempdir().unwrap();
    let engine = start_engine_with(&directory.path().join("patchouli.db"), DEFAULT).await;
    let refs = ["reject-a", "reject-b"].map(|id| EntityRef {
        entity_type: "knowledge".to_owned(),
        id: id.to_owned(),
    });
    let mut bases = Vec::new();
    for entity_ref in &refs {
        let created = engine
            .create(RpcParams {
                meta: meta([]),
                data: CreateEntityData {
                    entity_type: entity_ref.entity_type.clone(),
                    id: Some(entity_ref.id.clone()),
                    value: knowledge("base", &entity_ref.id),
                },
            })
            .await
            .unwrap();
        bases.push(version(&created.data.entity));
    }

    let mut staged = Vec::new();
    for (entity_ref, base) in refs.iter().zip(&bases) {
        let mutation = engine
            .update(RpcParams {
                meta: work_unit_meta(
                    "reject-all",
                    false,
                    [
                        ("base_versions", json!([base])),
                        ("conflict_strategy", json!("reject")),
                    ],
                ),
                data: UpdateEntityData {
                    entity_ref: entity_ref.clone(),
                    value: knowledge("staged", &entity_ref.id),
                },
            })
            .await
            .unwrap();
        staged.push(version(&mutation.data.entity));
    }
    for (entity_ref, base) in refs.iter().zip(&bases) {
        engine
            .update(RpcParams {
                meta: meta([("base_versions", json!([base]))]),
                data: UpdateEntityData {
                    entity_ref: entity_ref.clone(),
                    value: knowledge("outside", &entity_ref.id),
                },
            })
            .await
            .unwrap();
    }

    let error = engine
        .update(RpcParams {
            meta: work_unit_meta(
                "reject-all",
                true,
                [
                    ("base_versions", json!([staged[0]])),
                    ("conflict_strategy", json!("reject")),
                ],
            ),
            data: UpdateEntityData {
                entity_ref: refs[0].clone(),
                value: knowledge("closing", "reject-a"),
            },
        })
        .await
        .unwrap_err();
    assert_eq!(error.reason, BackendErrorReason::VersionConflict);
    assert_eq!(error.conflicts.len(), 2);
    let ids = error
        .conflicts
        .iter()
        .map(|conflict| conflict.entity_ref.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["reject-b", "reject-a"]);
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn work_unit_resolves_stale_member_writes_before_atomic_publication() {
    let directory = tempfile::tempdir().unwrap();
    let engine = start_engine_with(&directory.path().join("patchouli.db"), DEFAULT).await;
    let entity_ref = EntityRef {
        entity_type: "knowledge".to_owned(),
        id: "member-conflict".to_owned(),
    };
    let created = engine
        .create(RpcParams {
            meta: meta([]),
            data: CreateEntityData {
                entity_type: entity_ref.entity_type.clone(),
                id: Some(entity_ref.id.clone()),
                value: knowledge("some value", "base"),
            },
        })
        .await
        .unwrap();
    let base = version(&created.data.entity);

    engine
        .update(RpcParams {
            meta: work_unit_meta("work-merge", false, [("base_versions", json!([base]))]),
            data: UpdateEntityData {
                entity_ref: entity_ref.clone(),
                value: knowledge("some day", "first"),
            },
        })
        .await
        .unwrap();
    engine
        .update(RpcParams {
            meta: work_unit_meta("work-merge", true, [("base_versions", json!([base]))]),
            data: UpdateEntityData {
                entity_ref: entity_ref.clone(),
                value: knowledge("another value", "second"),
            },
        })
        .await
        .unwrap();

    let visible = engine
        .read(RpcParams {
            meta: meta([]),
            data: ReadEntityData { entity_ref },
        })
        .await
        .unwrap();
    assert_eq!(visible.data.state, ReadState::Conflicted);
    assert_eq!(visible.data.variants.len(), 2);
    assert!(visible.data.variants.iter().all(|variant| match variant {
        EntityVersion::Active { value, .. } => {
            value.pointer("/content/text") == Some(&json!("another day"))
        }
        EntityVersion::Deleted { .. } => false,
    }));
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn work_unit_uses_one_global_baseline_for_entities_first_read_later() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("patchouli.db");
    let engine = start_engine_with(&path, DEFAULT).await;
    let first_ref = EntityRef {
        entity_type: "knowledge".to_owned(),
        id: "baseline-first".to_owned(),
    };
    let second_ref = EntityRef {
        entity_type: "knowledge".to_owned(),
        id: "baseline-second".to_owned(),
    };
    let first = engine
        .create(RpcParams {
            meta: meta([]),
            data: CreateEntityData {
                entity_type: first_ref.entity_type.clone(),
                id: Some(first_ref.id.clone()),
                value: knowledge("first base", "base"),
            },
        })
        .await
        .unwrap();
    let second = engine
        .create(RpcParams {
            meta: meta([]),
            data: CreateEntityData {
                entity_type: second_ref.entity_type.clone(),
                id: Some(second_ref.id.clone()),
                value: knowledge("second base", "base"),
            },
        })
        .await
        .unwrap();
    let first_base = version(&first.data.entity);
    let second_base = version(&second.data.entity);

    engine.shutdown().await.unwrap();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute("DELETE FROM patchouli_change", [])
        .unwrap();
    drop(connection);
    let engine = start_engine_with(&path, DEFAULT).await;

    engine
        .read(RpcParams {
            meta: work_unit_meta("global-baseline", false, []),
            data: ReadEntityData {
                entity_ref: first_ref.clone(),
            },
        })
        .await
        .unwrap();
    let outside = engine
        .update(RpcParams {
            meta: meta([("base_versions", json!([second_base]))]),
            data: UpdateEntityData {
                entity_ref: second_ref.clone(),
                value: knowledge("second outside", "outside"),
            },
        })
        .await
        .unwrap();
    let outside_version = version(&outside.data.entity);

    let fixed = engine
        .read(RpcParams {
            meta: work_unit_meta("global-baseline", false, []),
            data: ReadEntityData {
                entity_ref: second_ref.clone(),
            },
        })
        .await
        .unwrap();
    assert_eq!(fixed.data.variants.len(), 1);
    assert_eq!(version(&fixed.data.variants[0]), second_base);

    engine
        .update(RpcParams {
            meta: work_unit_meta(
                "global-baseline",
                true,
                [("base_versions", json!([first_base]))],
            ),
            data: UpdateEntityData {
                entity_ref: first_ref,
                value: knowledge("first committed", "inside"),
            },
        })
        .await
        .unwrap();
    let visible = engine
        .read(RpcParams {
            meta: meta([]),
            data: ReadEntityData {
                entity_ref: second_ref,
            },
        })
        .await
        .unwrap();
    assert_eq!(version(&visible.data.variants[0]), outside_version);
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn work_unit_retrieval_uses_the_shared_baseline_and_staged_overlay() {
    let directory = tempfile::tempdir().unwrap();
    let engine = start_engine(&directory.path().join("patchouli.db")).await;
    let staged_ref = EntityRef {
        entity_type: "knowledge".to_owned(),
        id: "retrieval-a-staged".to_owned(),
    };
    let outside_ref = EntityRef {
        entity_type: "knowledge".to_owned(),
        id: "retrieval-b-outside".to_owned(),
    };
    let staged_base = engine
        .create(RpcParams {
            meta: meta([]),
            data: CreateEntityData {
                entity_type: staged_ref.entity_type.clone(),
                id: Some(staged_ref.id.clone()),
                value: knowledge("staged base", "staged-base"),
            },
        })
        .await
        .unwrap();
    let outside_base = engine
        .create(RpcParams {
            meta: meta([]),
            data: CreateEntityData {
                entity_type: outside_ref.entity_type.clone(),
                id: Some(outside_ref.id.clone()),
                value: knowledge("outside base", "outside-base"),
            },
        })
        .await
        .unwrap();
    let staged_base = version(&staged_base.data.entity);
    let outside_base = version(&outside_base.data.entity);

    engine
        .read(RpcParams {
            meta: work_unit_meta("retrieval-view", false, []),
            data: ReadEntityData {
                entity_ref: staged_ref.clone(),
            },
        })
        .await
        .unwrap();
    engine
        .update(RpcParams {
            meta: work_unit_meta(
                "retrieval-view",
                false,
                [("base_versions", json!([staged_base]))],
            ),
            data: UpdateEntityData {
                entity_ref: staged_ref.clone(),
                value: knowledge("staged overlay", "staged-overlay"),
            },
        })
        .await
        .unwrap();
    engine
        .update(RpcParams {
            meta: meta([("base_versions", json!([outside_base]))]),
            data: UpdateEntityData {
                entity_ref: outside_ref.clone(),
                value: knowledge("outside current", "outside-current"),
            },
        })
        .await
        .unwrap();

    let retrieved = engine
        .retrieve(RpcParams {
            meta: work_unit_meta("retrieval-view", false, []),
            data: RetrieveEntitiesData {
                query: json!({
                    "ids": [staged_ref.id, outside_ref.id],
                    "order": "id_asc"
                })
                .to_string(),
                types: Some(vec!["knowledge".to_owned()]),
                limit: 10,
            },
        })
        .await
        .unwrap();
    let values = retrieved
        .data
        .hits
        .iter()
        .map(|hit| match &hit.variants[0] {
            EntityVersion::Active {
                entity_ref, value, ..
            } => (
                entity_ref.id.as_str(),
                value
                    .pointer("/content/text")
                    .and_then(Value::as_str)
                    .unwrap(),
            ),
            EntityVersion::Deleted { .. } => unreachable!(),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        values,
        [
            ("retrieval-a-staged", "staged overlay"),
            ("retrieval-b-outside", "outside base"),
        ]
    );
    engine.shutdown().await.unwrap();
}

async fn start_engine(path: &std::path::Path) -> BackendEngine {
    start_engine_with(path, DEFAULT).await
}

async fn start_engine_with(path: &std::path::Path, config: &str) -> BackendEngine {
    let provider = Arc::new(SqliteProvider::open(path).await.unwrap());
    BackendEngine::start(BackendConfig::from_json(config).unwrap(), provider)
        .await
        .unwrap()
}

fn meta<const N: usize>(fields: [(&str, Value); N]) -> BTreeMap<String, Value> {
    let mut meta = BTreeMap::from([
        ("workspace_id".to_owned(), json!("workspace-1")),
        ("user_id".to_owned(), json!("user-7")),
        ("channel_id".to_owned(), json!("channel-7")),
    ]);
    meta.extend(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value)),
    );
    meta
}

fn work_unit_meta<const N: usize>(
    identity: &str,
    close: bool,
    fields: [(&str, Value); N],
) -> BTreeMap<String, Value> {
    let mut meta = meta(fields);
    meta.insert("transaction_id".to_owned(), json!(identity));
    if close {
        meta.insert("transaction_state".to_owned(), json!("commit"));
    }
    meta
}

fn knowledge(text: &str, source: &str) -> Value {
    let mut value: Value = serde_json::from_str(KNOWLEDGE).unwrap();
    value["content"] = json!({ "kind": "text", "text": text });
    value["metadata"]["extensions"] = json!({
        "test.source": { "source": source }
    });
    value
}

fn hit_ids(hits: &[patchouli_backend::RetrievalHit]) -> Vec<&str> {
    hits.iter()
        .map(|hit| match &hit.variants[0] {
            EntityVersion::Active { entity_ref, .. }
            | EntityVersion::Deleted { entity_ref, .. } => entity_ref.id.as_str(),
        })
        .collect()
}

fn version(entity: &EntityVersion) -> String {
    match entity {
        EntityVersion::Active { version, .. } | EntityVersion::Deleted { version, .. } => {
            version.clone()
        }
    }
}
