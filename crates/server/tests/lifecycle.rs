use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use async_trait::async_trait;
use patchouli_backend::{BackendConfig, BackendEngine, CreateEntityData, RpcParams};
use patchouli_provider::{Provider, ProviderError, ProviderRecovery};
use patchouli_server::{IpcError, LocalClient, LocalServer, ServerOptions};
use serde_json::json;

const EXAMPLE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../config/patchouli.example.json"
));

#[derive(Default)]
struct ProviderState {
    checkpoints: AtomicU64,
    shut_down: AtomicBool,
}

struct HealthyProvider(Arc<ProviderState>);

#[async_trait]
impl Provider for HealthyProvider {
    fn kind(&self) -> &'static str {
        "test"
    }

    async fn initialize(&self) -> Result<ProviderRecovery, ProviderError> {
        Ok(ProviderRecovery {
            generation: 7,
            recovered_after_unclean_shutdown: true,
        })
    }

    async fn health_check(&self) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn checkpoint(&self) -> Result<(), ProviderError> {
        self.0.checkpoints.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), ProviderError> {
        self.0.shut_down.store(true, Ordering::Relaxed);
        Ok(())
    }
}

#[tokio::test]
async fn daemon_accepts_status_and_shutdown_over_local_ipc() {
    let (_directory, endpoint) = test_endpoint();
    let provider_state = Arc::new(ProviderState::default());
    let provider = Arc::new(HealthyProvider(Arc::clone(&provider_state)));
    let engine = Arc::new(
        BackendEngine::start(
            BackendConfig::from_json(EXAMPLE).expect("valid config"),
            provider,
        )
        .await
        .expect("start engine"),
    );
    let server = LocalServer::bind(
        ServerOptions {
            endpoint: endpoint.clone(),
            node_id: "node-test".to_owned(),
            cluster_id: "cluster-test".to_owned(),
        },
        engine,
    )
    .await
    .expect("bind daemon");
    let task = tokio::spawn(server.run());

    let mut client = LocalClient::connect(&endpoint, "test-client", "1.0.0")
        .await
        .expect("connect client");
    let _idle_client = LocalClient::connect(&endpoint, "idle-client", "1.0.0")
        .await
        .expect("connect idle client");
    let status = client.status().await.expect("read status");
    assert!(status.data.ready);
    assert_eq!(status.data.provider, "test");
    assert_eq!(status.data.generation, 7);
    assert!(status.data.recovered_after_unclean_shutdown);
    assert_eq!(status.data.pid, std::process::id());
    assert_eq!(status.data.active_connections, 2);

    let checkpoint = client.checkpoint().await.expect("checkpoint provider");
    assert!(checkpoint.data.completed);
    assert_eq!(provider_state.checkpoints.load(Ordering::Relaxed), 1);

    let error = client
        .create(&RpcParams {
            meta: Default::default(),
            data: CreateEntityData {
                entity_type: "event".to_owned(),
                id: Some("event-1".to_owned()),
                value: json!({ "payload": "hello" }),
            },
        })
        .await
        .expect_err("CRUD is routed to the engine placeholder");
    assert!(matches!(error, IpcError::Rpc { code: -32006, .. }));

    let stopped = client.shutdown().await.expect("request shutdown");
    assert!(stopped.data.accepted);
    task.await.expect("server task").expect("server shutdown");
    assert!(provider_state.shut_down.load(Ordering::Relaxed));

    #[cfg(unix)]
    assert!(!std::path::Path::new(&endpoint).exists());
}

#[cfg(unix)]
fn test_endpoint() -> (Option<tempfile::TempDir>, String) {
    let directory = tempfile::tempdir().expect("temporary directory");
    let endpoint = directory
        .path()
        .join("patchouli.sock")
        .to_string_lossy()
        .into_owned();
    (Some(directory), endpoint)
}

#[cfg(windows)]
fn test_endpoint() -> (Option<tempfile::TempDir>, String) {
    (
        None,
        format!(
            r"\\.\pipe\patchouli-test-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("lifecycle")
        ),
    )
}
