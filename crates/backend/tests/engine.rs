use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use async_trait::async_trait;
use patchouli_backend::{
    BackendConfig, BackendEngine, BackendErrorReason, BackendService, CreateEntityData, RpcParams,
};
use patchouli_provider::{
    ConsistentRead, EntityCommit, EntityCommitOutcome, EntityKey, EntitySnapshot, Provider,
    ProviderCapabilities, ProviderError, ProviderRecovery, ReadConsistency, WorkUnit,
    WorkUnitCommit, WorkUnitCommitOutcome, WorkUnitPublish, WorkUnitReadOutcome,
};

const EXAMPLE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../config/patchouli.example.json"
));
const KNOWLEDGE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packages/protocol/schemas/examples/knowledge@1.json"
));

struct HealthyProvider(bool);
struct UnhealthyProvider;
struct FailedHealthProvider(Arc<AtomicBool>);

#[async_trait]
impl Provider for HealthyProvider {
    fn kind(&self) -> &'static str {
        "test"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            authority: true,
            replica: true,
            change_stream: true,
            retrieval: true,
            idempotency: true,
            work_units: true,
            causal_reads: true,
            monotonic_reads: true,
            read_your_writes: true,
            linearizable_reads: self.0,
        }
    }

    async fn initialize(&self) -> Result<ProviderRecovery, ProviderError> {
        Ok(ProviderRecovery {
            generation: 1,
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
        Err(ProviderError::new("test provider has no storage"))
    }

    async fn commit_entity(
        &self,
        _commit: EntityCommit,
    ) -> Result<EntityCommitOutcome, ProviderError> {
        Err(ProviderError::new("test provider has no storage"))
    }

    async fn read_entity_in_work_unit(
        &self,
        _work_unit: &WorkUnit,
        _key: &EntityKey,
        _consistency: ReadConsistency,
    ) -> Result<WorkUnitReadOutcome, ProviderError> {
        Err(ProviderError::new("test provider has no storage"))
    }

    async fn commit_entity_in_work_unit(
        &self,
        _commit: WorkUnitCommit,
    ) -> Result<WorkUnitCommitOutcome, ProviderError> {
        Err(ProviderError::new("test provider has no storage"))
    }

    async fn publish_work_unit(
        &self,
        _publish: WorkUnitPublish,
    ) -> Result<WorkUnitCommitOutcome, ProviderError> {
        Err(ProviderError::new("test provider has no storage"))
    }

    async fn checkpoint(&self) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), ProviderError> {
        Ok(())
    }
}

#[async_trait]
impl Provider for UnhealthyProvider {
    fn kind(&self) -> &'static str {
        "test"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        HealthyProvider(true).capabilities()
    }

    async fn initialize(&self) -> Result<ProviderRecovery, ProviderError> {
        Err(ProviderError::new("test provider is unavailable"))
    }

    async fn health_check(&self) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn read_entity(
        &self,
        _key: &EntityKey,
        _consistency: ReadConsistency,
    ) -> Result<ConsistentRead<Option<EntitySnapshot>>, ProviderError> {
        Err(ProviderError::new("test provider has no storage"))
    }

    async fn commit_entity(
        &self,
        _commit: EntityCommit,
    ) -> Result<EntityCommitOutcome, ProviderError> {
        Err(ProviderError::new("test provider has no storage"))
    }

    async fn read_entity_in_work_unit(
        &self,
        _work_unit: &WorkUnit,
        _key: &EntityKey,
        _consistency: ReadConsistency,
    ) -> Result<WorkUnitReadOutcome, ProviderError> {
        Err(ProviderError::new("test provider has no storage"))
    }

    async fn commit_entity_in_work_unit(
        &self,
        _commit: WorkUnitCommit,
    ) -> Result<WorkUnitCommitOutcome, ProviderError> {
        Err(ProviderError::new("test provider has no storage"))
    }

    async fn publish_work_unit(
        &self,
        _publish: WorkUnitPublish,
    ) -> Result<WorkUnitCommitOutcome, ProviderError> {
        Err(ProviderError::new("test provider has no storage"))
    }

    async fn checkpoint(&self) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), ProviderError> {
        Ok(())
    }
}

#[async_trait]
impl Provider for FailedHealthProvider {
    fn kind(&self) -> &'static str {
        "failed-health"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        HealthyProvider(true).capabilities()
    }

    async fn initialize(&self) -> Result<ProviderRecovery, ProviderError> {
        Ok(ProviderRecovery {
            generation: 1,
            recovered_after_unclean_shutdown: false,
        })
    }

    async fn health_check(&self) -> Result<(), ProviderError> {
        Err(ProviderError::new("unhealthy after initialization"))
    }

    async fn read_entity(
        &self,
        _key: &EntityKey,
        _consistency: ReadConsistency,
    ) -> Result<ConsistentRead<Option<EntitySnapshot>>, ProviderError> {
        Err(ProviderError::new("test provider has no storage"))
    }

    async fn commit_entity(
        &self,
        _commit: EntityCommit,
    ) -> Result<EntityCommitOutcome, ProviderError> {
        Err(ProviderError::new("test provider has no storage"))
    }

    async fn read_entity_in_work_unit(
        &self,
        _work_unit: &WorkUnit,
        _key: &EntityKey,
        _consistency: ReadConsistency,
    ) -> Result<WorkUnitReadOutcome, ProviderError> {
        Err(ProviderError::new("test provider has no storage"))
    }

    async fn commit_entity_in_work_unit(
        &self,
        _commit: WorkUnitCommit,
    ) -> Result<WorkUnitCommitOutcome, ProviderError> {
        Err(ProviderError::new("test provider has no storage"))
    }

    async fn publish_work_unit(
        &self,
        _publish: WorkUnitPublish,
    ) -> Result<WorkUnitCommitOutcome, ProviderError> {
        Err(ProviderError::new("test provider has no storage"))
    }

    async fn checkpoint(&self) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), ProviderError> {
        self.0.store(true, Ordering::Relaxed);
        Ok(())
    }
}

#[tokio::test]
async fn engine_starts_with_validated_config_and_routes_to_provider() {
    let engine = BackendEngine::start(
        BackendConfig::from_json(EXAMPLE).expect("valid config"),
        Arc::new(HealthyProvider(true)),
    )
    .await
    .expect("start engine");

    assert_eq!(engine.provider_kind(), "test");
    assert_eq!(engine.recovery().generation, 1);
    assert!(engine.config().entity_types.contains_key("artifact"));
    assert!(engine.config().entity_types.contains_key("knowledge"));
    assert!(
        engine
            .config()
            .entity_types
            .contains_key("knowledge_relation")
    );

    let error = engine
        .create(RpcParams {
            meta: Default::default(),
            data: CreateEntityData {
                entity_type: "knowledge".to_owned(),
                id: Some("knowledge-1".to_owned()),
                value: serde_json::from_str(KNOWLEDGE).expect("valid knowledge fixture"),
            },
        })
        .await
        .expect_err("test provider has no storage");
    assert_eq!(error.reason, BackendErrorReason::InvalidRequest);
}

#[tokio::test]
async fn engine_does_not_start_with_an_unhealthy_provider() {
    let result = BackendEngine::start(
        BackendConfig::from_json(EXAMPLE).expect("valid config"),
        Arc::new(UnhealthyProvider),
    )
    .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn engine_rejects_a_provider_without_effective_linearizable_read_support() {
    let error = match BackendEngine::start(
        BackendConfig::from_json(EXAMPLE).expect("valid config"),
        Arc::new(HealthyProvider(false)),
    )
    .await
    {
        Ok(_) => panic!("linearizable configuration must require provider support"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("requires linearizable reads"));
}

#[tokio::test]
async fn engine_cleans_up_when_post_initialize_health_check_fails() {
    let shut_down = Arc::new(AtomicBool::new(false));
    let result = BackendEngine::start(
        BackendConfig::from_json(EXAMPLE).expect("valid config"),
        Arc::new(FailedHealthProvider(Arc::clone(&shut_down))),
    )
    .await;

    assert!(result.is_err());
    assert!(shut_down.load(Ordering::Relaxed));
}
