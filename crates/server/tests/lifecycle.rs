use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use async_trait::async_trait;
use patchouli_backend::{
    BackendConfig, BackendEngine, CreateEntityData, RpcParams, SubscribeChangesData,
    UnsubscribeChangesData,
};
use patchouli_provider::{
    EntityCommit, EntityCommitOutcome, EntityKey, EntitySnapshot, Provider, ProviderCapabilities,
    ProviderError, ProviderRecovery, WorkUnit, WorkUnitCommit, WorkUnitCommitOutcome,
    WorkUnitPublish, WorkUnitReadOutcome,
};
use patchouli_provider_sqlite::SqliteProvider;
use patchouli_server::{IpcError, LocalClient, LocalServer, ServerOptions};

const EXAMPLE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../config/patchouli.example.json"
));
const KNOWLEDGE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packages/protocol/schemas/examples/knowledge@1.json"
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

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            authority: true,
            replica: true,
            change_stream: true,
            retrieval: true,
            idempotency: true,
            work_units: true,
            causal_sessions: true,
        }
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

    async fn read_entity(&self, _key: &EntityKey) -> Result<Option<EntitySnapshot>, ProviderError> {
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

    let (mut client, handshake) = LocalClient::connect_with_capabilities(
        &endpoint,
        "test-client",
        "1.0.0",
        vec!["unknown".to_owned(), "subscriptions".to_owned()],
    )
    .await
    .expect("connect client");
    assert_eq!(handshake.capabilities, vec!["subscriptions"]);
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
            meta: std::collections::BTreeMap::from([
                ("workspace_id".to_owned(), serde_json::json!("workspace-1")),
                ("user_id".to_owned(), serde_json::json!("user-7")),
                ("channel_id".to_owned(), serde_json::json!("channel-7")),
            ]),
            data: CreateEntityData {
                entity_type: "knowledge".to_owned(),
                id: Some("knowledge-1".to_owned()),
                value: serde_json::from_str(KNOWLEDGE).expect("valid knowledge fixture"),
            },
        })
        .await
        .expect_err("CRUD is routed to the provider");
    assert!(matches!(error, IpcError::Rpc { code: -32009, .. }));

    let stopped = client.shutdown().await.expect("request shutdown");
    assert!(stopped.data.accepted);
    task.await.expect("server task").expect("server shutdown");
    assert!(provider_state.shut_down.load(Ordering::Relaxed));

    #[cfg(unix)]
    assert!(!std::path::Path::new(&endpoint).exists());
}

#[tokio::test]
async fn daemon_streams_committed_changes_and_unsubscribes_over_ipc() {
    let (_endpoint_directory, endpoint) = test_endpoint();
    let database_directory = tempfile::tempdir().expect("temporary database directory");
    let database = database_directory.path().join("patchouli.db");
    let provider = Arc::new(SqliteProvider::open(&database).await.expect("open SQLite"));
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
            node_id: "node-stream".to_owned(),
            cluster_id: "cluster-stream".to_owned(),
        },
        engine,
    )
    .await
    .expect("bind daemon");
    let task = tokio::spawn(server.run());
    let (mut subscriber, _) = LocalClient::connect_with_capabilities(
        &endpoint,
        "subscriber",
        "1.0.0",
        vec!["subscriptions".to_owned()],
    )
    .await
    .expect("connect subscriber");
    let subscription = subscriber
        .subscribe(&RpcParams {
            meta: std::collections::BTreeMap::from([
                ("workspace_id".to_owned(), serde_json::json!("workspace-1")),
                ("user_id".to_owned(), serde_json::json!("user-7")),
            ]),
            data: SubscribeChangesData {
                filter: None,
                after_cursor: None,
            },
        })
        .await
        .expect("subscribe");
    let mut writer = LocalClient::connect(&endpoint, "writer", "1.0.0")
        .await
        .expect("connect writer");
    writer
        .create(&RpcParams {
            meta: std::collections::BTreeMap::from([
                ("workspace_id".to_owned(), serde_json::json!("workspace-1")),
                ("user_id".to_owned(), serde_json::json!("user-7")),
                ("channel_id".to_owned(), serde_json::json!("channel-7")),
            ]),
            data: CreateEntityData {
                entity_type: "knowledge".to_owned(),
                id: Some("streamed-knowledge".to_owned()),
                value: serde_json::from_str(KNOWLEDGE).expect("valid knowledge fixture"),
            },
        })
        .await
        .expect("create entity");
    let event = subscriber.next_change().await.expect("receive change");
    assert_eq!(event.data.change.entity_ref.id, "streamed-knowledge");
    let removed = subscriber
        .unsubscribe(&RpcParams {
            meta: Default::default(),
            data: UnsubscribeChangesData {
                subscription_id: subscription.data.subscription_id,
            },
        })
        .await
        .expect("unsubscribe");
    assert!(removed.data.removed);
    writer.shutdown().await.expect("request shutdown");
    task.await.expect("server task").expect("server shutdown");
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
