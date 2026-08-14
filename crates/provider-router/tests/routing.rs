use std::{collections::BTreeMap, sync::Arc};

use patchouli_provider::{
    EntityCommit, EntityCommitOutcome, EntityKey, Provider, StoredChangeKind, StoredEntityVersion,
    StoredVersionState,
};
use patchouli_provider_router::{RoutingProvider, ScopeRoute};
use patchouli_provider_sqlite::SqliteProvider;
use serde_json::json;

#[tokio::test]
async fn routes_each_scope_to_one_provider() {
    let directory = tempfile::tempdir().unwrap();
    let local: Arc<dyn Provider> = Arc::new(
        SqliteProvider::open(directory.path().join("local.db"))
            .await
            .unwrap(),
    );
    let remote: Arc<dyn Provider> = Arc::new(
        SqliteProvider::open(directory.path().join("remote.db"))
            .await
            .unwrap(),
    );
    let mut providers = BTreeMap::new();
    providers.insert("local".to_owned(), Arc::clone(&local));
    providers.insert("shared".to_owned(), Arc::clone(&remote));
    let router = RoutingProvider::new(
        providers,
        "local".to_owned(),
        vec![ScopeRoute {
            scope: BTreeMap::from([("workspace_id".to_owned(), json!("shared"))]),
            provider: "shared".to_owned(),
        }],
    )
    .unwrap();
    router.initialize().await.unwrap();

    let local_key = key("local", "local-entity");
    let remote_key = key("shared", "remote-entity");
    assert_eq!(
        router
            .commit_entity(commit(local_key.clone()))
            .await
            .unwrap(),
        EntityCommitOutcome::Committed
    );
    assert_eq!(
        router
            .commit_entity(commit(remote_key.clone()))
            .await
            .unwrap(),
        EntityCommitOutcome::Committed
    );

    assert!(local.read_entity(&local_key).await.unwrap().is_some());
    assert!(local.read_entity(&remote_key).await.unwrap().is_none());
    assert!(remote.read_entity(&remote_key).await.unwrap().is_some());
    assert!(remote.read_entity(&local_key).await.unwrap().is_none());
    router.shutdown().await.unwrap();
}

fn key(workspace: &str, id: &str) -> EntityKey {
    EntityKey {
        scope_json: serde_json::to_string(&BTreeMap::from([("workspace_id", workspace)])).unwrap(),
        entity_type: "knowledge".to_owned(),
        entity_id: id.to_owned(),
    }
}

fn commit(key: EntityKey) -> EntityCommit {
    EntityCommit {
        key,
        expected_heads: Vec::new(),
        new_versions: vec![StoredEntityVersion {
            version: "v1".to_owned(),
            state: StoredVersionState::Active,
            value_json: Some(r#"{"content":"value"}"#.to_owned()),
            crdt_fields: Vec::new(),
        }],
        head_versions: vec!["v1".to_owned()],
        change_kind: StoredChangeKind::Created,
        causal_token: "token-1".to_owned(),
        event_meta_json: "{}".to_owned(),
        session_keys: Vec::new(),
        recorded_at_unix_ms: 1,
        deadline_unix_ms: None,
    }
}
