use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use patchouli_provider::{
    ConsistentRead, EntityCommit, EntityCommitOutcome, EntityKey, EntitySnapshot, Provider,
    ProviderError, ProviderRecovery, ReadConsistency, StoredChangeKind, StoredEntityVersion,
    StoredVersionState, WorkUnit, WorkUnitCommit, WorkUnitCommitOutcome, WorkUnitPublish,
    WorkUnitReadOutcome,
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

    assert!(matches!(
        local
            .read_entity(&local_key, ReadConsistency::authority())
            .await
            .unwrap(),
        ConsistentRead::Read { value: Some(_), .. }
    ));
    assert!(matches!(
        local
            .read_entity(&remote_key, ReadConsistency::authority())
            .await
            .unwrap(),
        ConsistentRead::Read { value: None, .. }
    ));
    assert!(matches!(
        remote
            .read_entity(&remote_key, ReadConsistency::authority())
            .await
            .unwrap(),
        ConsistentRead::Read { value: Some(_), .. }
    ));
    assert!(matches!(
        remote
            .read_entity(&local_key, ReadConsistency::authority())
            .await
            .unwrap(),
        ConsistentRead::Read { value: None, .. }
    ));
    router.shutdown().await.unwrap();
}

#[tokio::test]
async fn lifecycle_errors_identify_the_provider() {
    let directory = tempfile::tempdir().unwrap();
    let local: Arc<dyn Provider> = Arc::new(
        SqliteProvider::open(directory.path().join("local.db"))
            .await
            .unwrap(),
    );
    let shared: Arc<dyn Provider> = Arc::new(
        SqliteProvider::open(directory.path().join("shared.db"))
            .await
            .unwrap(),
    );
    let router = RoutingProvider::new(
        BTreeMap::from([
            ("local".to_owned(), Arc::clone(&local)),
            ("shared".to_owned(), Arc::clone(&shared)),
        ]),
        "local".to_owned(),
        Vec::new(),
    )
    .unwrap();
    router.initialize().await.unwrap();
    shared.shutdown().await.unwrap();

    let health = router.health_check().await.unwrap_err();
    assert!(
        health
            .to_string()
            .contains("provider \"shared\" health check failed")
    );
    let checkpoint = router.checkpoint().await.unwrap_err();
    assert!(
        checkpoint
            .to_string()
            .contains("provider \"shared\" checkpoint failed")
    );
    router.shutdown().await.unwrap();
}

#[tokio::test]
async fn shutdown_errors_identify_the_provider() {
    let provider: Arc<dyn Provider> = Arc::new(FailingShutdownProvider);
    let router = RoutingProvider::new(
        BTreeMap::from([("broken".to_owned(), provider)]),
        "broken".to_owned(),
        Vec::new(),
    )
    .unwrap();

    let error = router.shutdown().await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("provider \"broken\" shutdown failed")
    );
}

struct FailingShutdownProvider;

#[async_trait]
impl Provider for FailingShutdownProvider {
    fn kind(&self) -> &'static str {
        "test"
    }

    async fn initialize(&self) -> Result<ProviderRecovery, ProviderError> {
        Ok(ProviderRecovery {
            generation: 0,
            recovered_after_unclean_shutdown: false,
        })
    }

    async fn health_check(&self) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn read_entity(
        &self,
        _key: &EntityKey,
        _consistency: ReadConsistency,
    ) -> Result<ConsistentRead<Option<EntitySnapshot>>, ProviderError> {
        unreachable!()
    }

    async fn commit_entity(
        &self,
        _commit: EntityCommit,
    ) -> Result<EntityCommitOutcome, ProviderError> {
        unreachable!()
    }

    async fn read_entity_in_work_unit(
        &self,
        _work_unit: &WorkUnit,
        _key: &EntityKey,
        _consistency: ReadConsistency,
    ) -> Result<WorkUnitReadOutcome, ProviderError> {
        unreachable!()
    }

    async fn commit_entity_in_work_unit(
        &self,
        _commit: WorkUnitCommit,
    ) -> Result<WorkUnitCommitOutcome, ProviderError> {
        unreachable!()
    }

    async fn publish_work_unit(
        &self,
        _publish: WorkUnitPublish,
    ) -> Result<WorkUnitCommitOutcome, ProviderError> {
        unreachable!()
    }

    async fn checkpoint(&self) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), ProviderError> {
        Err(ProviderError::new("test shutdown failure"))
    }
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
        write_session_keys: Vec::new(),
        ordering_key_json: None,
        recorded_at_unix_ms: 1,
        deadline_unix_ms: None,
    }
}
