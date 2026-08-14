use std::sync::Arc;

use async_trait::async_trait;
use patchouli_backend::{
    BackendConfig, BackendEngine, BackendErrorReason, BackendService, CreateEntityData, RpcParams,
};
use patchouli_provider::{Provider, ProviderError};
use serde_json::json;

const EXAMPLE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../config/patchouli.example.json"
));

struct HealthyProvider;
struct UnhealthyProvider;

#[async_trait]
impl Provider for HealthyProvider {
    fn kind(&self) -> &'static str {
        "test"
    }

    async fn health_check(&self) -> Result<(), ProviderError> {
        Ok(())
    }
}

#[async_trait]
impl Provider for UnhealthyProvider {
    fn kind(&self) -> &'static str {
        "test"
    }

    async fn health_check(&self) -> Result<(), ProviderError> {
        Err(ProviderError::new("test provider is unavailable"))
    }
}

#[tokio::test]
async fn engine_starts_with_validated_config_and_routes_to_placeholders() {
    let engine = BackendEngine::start(
        BackendConfig::from_json(EXAMPLE).expect("valid config"),
        Arc::new(HealthyProvider),
    )
    .await
    .expect("start engine");

    assert_eq!(engine.provider_kind(), "test");
    assert!(engine.config().entity_types.contains_key("event"));

    let error = engine
        .create(RpcParams {
            meta: Default::default(),
            data: CreateEntityData {
                entity_type: "event".to_owned(),
                id: Some("event-1".to_owned()),
                value: json!({ "payload": "hello" }),
            },
        })
        .await
        .expect_err("business logic is still a placeholder");
    assert_eq!(error.reason, BackendErrorReason::UnsupportedCapability);
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
