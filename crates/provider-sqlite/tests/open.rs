use patchouli_provider::{
    ChangeQuery, ConsistencyAcquireOutcome, ConsistencyQuery, EntityCommit, EntityCommitOutcome,
    EntityKey, Provider, StoredChangeKind, StoredCrdtChange, StoredCrdtField, StoredEntityVersion,
    StoredVersionState, WorkUnit, WorkUnitCommit, WorkUnitCommitOutcome, WorkUnitExpiryAction,
    WorkUnitReadOutcome,
};
use patchouli_provider_sqlite::SqliteProvider;
use tokio_rusqlite::rusqlite::{Connection, params};

const KNOWLEDGE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packages/protocol/schemas/examples/knowledge@1.json"
));
const RELATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packages/protocol/schemas/examples/knowledge-relation@1.json"
));

#[tokio::test]
async fn opens_a_database_and_reports_health() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("nested").join("patchouli.db");

    let provider = SqliteProvider::open(&path).await.expect("open SQLite");

    assert_eq!(provider.kind(), "sqlite");
    let recovery = provider.initialize().await.expect("initialize SQLite");
    assert_eq!(recovery.generation, 1);
    assert!(!recovery.recovered_after_unclean_shutdown);
    provider.health_check().await.expect("healthy SQLite");
    provider.checkpoint().await.expect("checkpoint SQLite");
    provider.shutdown().await.expect("shut down SQLite");
    assert!(path.exists());
}

#[tokio::test]
async fn change_reads_prune_records_outside_configured_retention() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let provider = SqliteProvider::open(directory.path().join("patchouli.db"))
        .await
        .expect("open SQLite");
    provider.initialize().await.expect("initialize SQLite");
    let scope = r#"{"workspace_id":"workspace-1"}"#.to_owned();
    provider
        .commit_entity(EntityCommit {
            key: EntityKey {
                scope_json: scope.clone(),
                entity_type: "knowledge".to_owned(),
                entity_id: "retained".to_owned(),
            },
            expected_heads: vec![],
            new_versions: vec![StoredEntityVersion {
                version: "v1".to_owned(),
                state: StoredVersionState::Active,
                value_json: Some(KNOWLEDGE.to_owned()),
                crdt_fields: vec![],
            }],
            head_versions: vec!["v1".to_owned()],
            change_kind: StoredChangeKind::Created,
            causal_token: "retention-token".to_owned(),
            event_meta_json: "{}".to_owned(),
            session_keys: vec![],
            recorded_at_unix_ms: 10,
        })
        .await
        .expect("commit entity");
    let page = provider
        .read_changes(ChangeQuery {
            scope_json: scope,
            entity_types: None,
            entity_ids: None,
            after_cursor: 0,
            limit: 10,
            retained_after_unix_ms: 11,
        })
        .await
        .expect("read retained changes");
    assert!(page.changes.is_empty());
    assert_eq!(page.oldest_cursor, None);
    assert_eq!(
        provider
            .acquire_consistency(ConsistencyQuery {
                scope_json: r#"{"workspace_id":"workspace-1"}"#.to_owned(),
                minimum_tokens: vec!["retention-token".to_owned()],
                session_keys: vec![r#"{"session":"one"}"#.to_owned()],
            })
            .await
            .expect("acquire retained causal frontier"),
        ConsistencyAcquireOutcome::Acquired {
            causal_token: Some("retention-token".to_owned()),
        }
    );
    provider.shutdown().await.expect("shutdown SQLite");
}

#[tokio::test]
async fn detects_unclean_and_clean_restarts() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("patchouli.db");

    let first = SqliteProvider::open(&path)
        .await
        .expect("open first SQLite");
    let first_recovery = first.initialize().await.expect("initialize first SQLite");
    assert!(!first_recovery.recovered_after_unclean_shutdown);
    drop(first);

    let second = SqliteProvider::open(&path)
        .await
        .expect("reopen after crash");
    let second_recovery = second.initialize().await.expect("recover SQLite");
    assert_eq!(second_recovery.generation, 2);
    assert!(second_recovery.recovered_after_unclean_shutdown);
    second.shutdown().await.expect("clean shutdown");
    drop(second);

    let third = SqliteProvider::open(&path)
        .await
        .expect("reopen after shutdown");
    let third_recovery = third.initialize().await.expect("initialize third SQLite");
    assert_eq!(third_recovery.generation, 3);
    assert!(!third_recovery.recovered_after_unclean_shutdown);
    third.shutdown().await.expect("final shutdown");
}

#[tokio::test]
async fn prevents_two_providers_from_owning_one_database() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("patchouli.db");
    let first = SqliteProvider::open(&path)
        .await
        .expect("open first SQLite");
    first.initialize().await.expect("initialize first SQLite");

    let second = SqliteProvider::open(&path).await;

    assert!(second.is_err());
    first.shutdown().await.expect("shut down first SQLite");
}

#[tokio::test]
async fn defines_generic_entries_and_typed_fact_views() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("patchouli.db");
    let provider = SqliteProvider::open(&path).await.expect("open SQLite");
    provider.initialize().await.expect("initialize SQLite");
    provider.shutdown().await.expect("shut down SQLite");

    let connection = Connection::open(&path).expect("open database for inspection");
    let schema_version: u32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read schema version");
    assert_eq!(schema_version, 9);

    insert_active_version(
        &connection,
        "knowledge",
        "knowledge-derived",
        "v1",
        KNOWLEDGE,
    );
    insert_active_version(
        &connection,
        "knowledge_relation",
        "relation-1",
        "v1",
        RELATION,
    );

    let knowledge_id: String = connection
        .query_row("SELECT knowledge_id FROM patchouli_knowledge", [], |row| {
            row.get(0)
        })
        .expect("query typed knowledge view");
    assert_eq!(knowledge_id, "knowledge-derived");

    let relation: (String, u32, u32, String, String) = connection
        .query_row(
            "SELECT
                relation_type,
                json_array_length(from_knowledge_refs_json),
                json_array_length(to_knowledge_refs_json),
                json_extract(from_knowledge_refs_json, '$[1].id'),
                json_extract(to_knowledge_refs_json, '$[1].id')
             FROM patchouli_knowledge_relation",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .expect("query typed relation view");
    assert_eq!(
        relation,
        (
            "derived_from".to_owned(),
            2,
            2,
            "knowledge-derived-b".to_owned(),
            "knowledge-source-b".to_owned()
        )
    );

    assert!(
        connection
            .execute(
                "INSERT INTO patchouli_entity_version (
                    scope_json,
                    entity_type,
                    entity_id,
                    version,
                    state,
                    value_json,
                    recorded_at_unix_ms
                 ) VALUES ('{}', 'knowledge', 'invalid-active', 'v1', 'active', NULL, 1)",
                [],
            )
            .is_err()
    );
    assert!(
        connection
            .execute(
                "INSERT INTO patchouli_entity_version (
                    scope_json,
                    entity_type,
                    entity_id,
                    version,
                    state,
                    value_json,
                    recorded_at_unix_ms
                 ) VALUES ('{}', 'knowledge', 'invalid-delete', 'v1', 'deleted', '{}', 1)",
                [],
            )
            .is_err()
    );
}

#[tokio::test]
async fn defines_crdt_change_graph_and_entity_frontiers() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("patchouli.db");
    let provider = SqliteProvider::open(&path).await.expect("open SQLite");
    provider.initialize().await.expect("initialize SQLite");
    provider.shutdown().await.expect("shut down SQLite");

    let connection = Connection::open(&path).expect("open database for inspection");
    insert_active_version(&connection, "knowledge", "knowledge-crdt", "v1", KNOWLEDGE);
    connection
        .execute(
            "INSERT INTO patchouli_crdt_change (change_hash, change_bytes)
             VALUES ('root', x'01'), ('branch', x'02')",
            [],
        )
        .expect("insert CRDT changes");
    connection
        .execute(
            "INSERT INTO patchouli_crdt_change_parent (change_hash, parent_hash)
             VALUES ('branch', 'root')",
            [],
        )
        .expect("insert CRDT dependency");
    connection
        .execute(
            "INSERT INTO patchouli_entity_crdt_head (
                scope_json,
                entity_type,
                entity_id,
                version,
                field_path,
                change_hash
             ) VALUES (
                '{\"channel_id\":\"channel-7\"}',
                'knowledge',
                'knowledge-crdt',
                'v1',
                '/content',
                'branch'
             )",
            [],
        )
        .expect("attach CRDT frontier");

    let frontier: (String, String) = connection
        .query_row(
            "SELECT field_path, change_hash FROM patchouli_entity_crdt_head",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("read CRDT frontier");
    assert_eq!(frontier, ("/content".to_owned(), "branch".to_owned()));
}

#[tokio::test]
async fn entity_commit_compares_heads_and_rolls_back_on_conflict() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("patchouli.db");
    let provider = SqliteProvider::open(&path).await.expect("open SQLite");
    provider.initialize().await.expect("initialize SQLite");
    let key = EntityKey {
        scope_json: r#"{"channel_id":"channel-7"}"#.to_owned(),
        entity_type: "knowledge".to_owned(),
        entity_id: "knowledge-cas".to_owned(),
    };
    let create = EntityCommit {
        key: key.clone(),
        expected_heads: vec![],
        new_versions: vec![StoredEntityVersion {
            version: "v1".to_owned(),
            state: StoredVersionState::Active,
            value_json: Some(KNOWLEDGE.to_owned()),
            crdt_fields: vec![],
        }],
        head_versions: vec!["v1".to_owned()],
        change_kind: StoredChangeKind::Created,
        causal_token: "causal-v1".to_owned(),
        event_meta_json: "{}".to_owned(),
        session_keys: vec![],
        recorded_at_unix_ms: 1,
    };
    assert_eq!(
        provider.commit_entity(create.clone()).await.unwrap(),
        EntityCommitOutcome::Committed
    );
    assert_eq!(
        provider.commit_entity(create).await.unwrap(),
        EntityCommitOutcome::Conflict {
            current_heads: vec!["v1".to_owned()]
        }
    );
    provider.shutdown().await.expect("shut down SQLite");

    let connection = Connection::open(path).expect("inspect SQLite");
    let counts: (u32, u32) = connection
        .query_row(
            "SELECT
                (SELECT count(*) FROM patchouli_entity_version),
                (SELECT count(*) FROM patchouli_change)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(counts, (1, 1));
}

#[tokio::test]
async fn expired_work_unit_discards_staged_versions_without_publication() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("patchouli.db");
    let provider = SqliteProvider::open(&path).await.expect("open SQLite");
    provider.initialize().await.expect("initialize SQLite");
    let key = EntityKey {
        scope_json: r#"{"channel_id":"channel-7"}"#.to_owned(),
        entity_type: "knowledge".to_owned(),
        entity_id: "knowledge-expired".to_owned(),
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let work_unit = WorkUnit {
        identity_json:
            r#"{"fields":{"transaction_id":"expired"},"scope":{"channel_id":"channel-7"}}"#
                .to_owned(),
        policy_json: r#"{"publication":"discard"}"#.to_owned(),
        expiry_action: WorkUnitExpiryAction::Discard,
        now_unix_ms: now,
        expires_at_unix_ms: now + 60_000,
    };
    assert_eq!(
        provider
            .read_entity_in_work_unit(&work_unit, &key)
            .await
            .unwrap(),
        WorkUnitReadOutcome::Open(None)
    );
    assert_eq!(
        provider
            .commit_entity_in_work_unit(WorkUnitCommit {
                work_unit: work_unit.clone(),
                entity: EntityCommit {
                    key: key.clone(),
                    expected_heads: vec![],
                    new_versions: vec![StoredEntityVersion {
                        version: "staged-v1".to_owned(),
                        state: StoredVersionState::Active,
                        value_json: Some(KNOWLEDGE.to_owned()),
                        crdt_fields: vec![StoredCrdtField {
                            path: "/content".to_owned(),
                            heads: vec!["staged-change".to_owned()],
                            changes: vec![StoredCrdtChange {
                                hash: "staged-change".to_owned(),
                                parents: vec![],
                                bytes: vec![1],
                            }],
                        }],
                    }],
                    head_versions: vec!["staged-v1".to_owned()],
                    change_kind: StoredChangeKind::Created,
                    causal_token: "causal-staged-v1".to_owned(),
                    event_meta_json: "{}".to_owned(),
                    session_keys: vec![],
                    recorded_at_unix_ms: now,
                },
                conflict_policy_json: r#"{"strategy":"mvcc"}"#.to_owned(),
                idempotency: None,
                close: false,
            })
            .await
            .unwrap(),
        WorkUnitCommitOutcome::Staged
    );
    Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE patchouli_work_unit
             SET opened_at_unix_ms = ?1, expires_at_unix_ms = ?2",
            params![now - 2, now - 1],
        )
        .unwrap();
    assert_eq!(provider.read_entity(&key).await.unwrap(), None);
    assert_eq!(
        provider
            .read_entity_in_work_unit(
                &WorkUnit {
                    now_unix_ms: now + 1,
                    expires_at_unix_ms: now + 60_001,
                    ..work_unit
                },
                &key,
            )
            .await
            .unwrap(),
        WorkUnitReadOutcome::Expired
    );
    provider.shutdown().await.expect("shut down SQLite");

    let connection = Connection::open(path).expect("inspect SQLite");
    let counts: (u32, u32, u32, String) = connection
        .query_row(
            "SELECT
                (SELECT count(*) FROM patchouli_entity_version),
                (SELECT count(*) FROM patchouli_change),
                (SELECT count(*) FROM patchouli_crdt_change),
                (SELECT state FROM patchouli_work_unit)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(counts, (0, 0, 0, "expired".to_owned()));
}

#[tokio::test]
async fn work_unit_identity_is_bound_to_its_opening_policy() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let provider = SqliteProvider::open(directory.path().join("patchouli.db"))
        .await
        .expect("open SQLite");
    provider.initialize().await.expect("initialize SQLite");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let key = EntityKey {
        scope_json: r#"{"channel_id":"channel-7"}"#.to_owned(),
        entity_type: "knowledge".to_owned(),
        entity_id: "policy-bound".to_owned(),
    };
    let work_unit = WorkUnit {
        identity_json:
            r#"{"fields":{"transaction_id":"policy"},"scope":{"channel_id":"channel-7"}}"#
                .to_owned(),
        policy_json: r#"{"ttl":60000}"#.to_owned(),
        expiry_action: WorkUnitExpiryAction::Discard,
        now_unix_ms: now,
        expires_at_unix_ms: now + 60_000,
    };
    assert_eq!(
        provider
            .read_entity_in_work_unit(&work_unit, &key)
            .await
            .unwrap(),
        WorkUnitReadOutcome::Open(None)
    );
    assert_eq!(
        provider
            .read_entity_in_work_unit(
                &WorkUnit {
                    policy_json: r#"{"ttl":120000}"#.to_owned(),
                    ..work_unit
                },
                &key,
            )
            .await
            .unwrap(),
        WorkUnitReadOutcome::PolicyMismatch
    );
    provider.shutdown().await.expect("shut down SQLite");
}

fn insert_active_version(
    connection: &Connection,
    entity_type: &str,
    entity_id: &str,
    version: &str,
    value_json: &str,
) {
    let scope = r#"{"channel_id":"channel-7"}"#;
    connection
        .execute(
            "INSERT INTO patchouli_entity_version (
                scope_json,
                entity_type,
                entity_id,
                version,
                state,
                value_json,
                recorded_at_unix_ms
             ) VALUES (?1, ?2, ?3, ?4, 'active', ?5, 1)",
            params![scope, entity_type, entity_id, version, value_json],
        )
        .expect("insert entity version");
    connection
        .execute(
            "INSERT INTO patchouli_entity_head (
                scope_json,
                entity_type,
                entity_id,
                version
             ) VALUES (?1, ?2, ?3, ?4)",
            params![scope, entity_type, entity_id, version],
        )
        .expect("insert entity head");
}
use std::time::{SystemTime, UNIX_EPOCH};
