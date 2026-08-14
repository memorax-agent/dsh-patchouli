use patchouli_provider::Provider;
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
    assert_eq!(schema_version, 3);

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
