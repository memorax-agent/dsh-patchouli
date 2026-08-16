use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use patchouli_backend::{
    ArtifactDownloadChunkData, ArtifactUploadBeginData, ArtifactUploadChunkData,
    ArtifactUploadCommitData, ArtifactValue, BackendConfig, BackendEngine, CreateEntityData,
    EntityVersion, RpcParams, SubscribeChangesData, UnsubscribeChangesData,
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
const MANAGED_ARTIFACT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packages/protocol/schemas/examples/artifact-managed@1.json"
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
    let artifact_directory = tempfile::tempdir().expect("temporary artifact directory");
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
            artifact_root: artifact_directory.path().join("artifacts"),
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

    let deadline_error = client
        .create(&RpcParams {
            meta: std::collections::BTreeMap::from([
                ("workspace_id".to_owned(), serde_json::json!("workspace-1")),
                ("user_id".to_owned(), serde_json::json!("user-7")),
                ("deadline_unix_ms".to_owned(), serde_json::json!(0)),
            ]),
            data: CreateEntityData {
                entity_type: "knowledge".to_owned(),
                id: Some("expired".to_owned()),
                value: serde_json::from_str(KNOWLEDGE).expect("valid knowledge fixture"),
            },
        })
        .await
        .expect_err("expired request must be rejected before the provider");
    assert!(matches!(deadline_error, IpcError::Rpc { code: -32007, .. }));

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
    let artifact_directory = tempfile::tempdir().expect("temporary artifact directory");
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
            artifact_root: artifact_directory.path().join("artifacts"),
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

#[tokio::test]
async fn daemon_uploads_and_downloads_scoped_managed_artifacts() {
    let (_endpoint_directory, endpoint) = test_endpoint();
    let storage_directory = tempfile::tempdir().expect("temporary storage directory");
    let database = storage_directory.path().join("patchouli.db");
    let artifact_root = storage_directory.path().join("artifacts");
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
            artifact_root,
            node_id: "node-artifact".to_owned(),
            cluster_id: "cluster-test".to_owned(),
        },
        engine,
    )
    .await
    .expect("bind daemon");
    let task = tokio::spawn(server.run());
    let (mut client, handshake) = LocalClient::connect_with_capabilities(
        &endpoint,
        "artifact-client",
        "1.0.0",
        vec!["artifacts".to_owned()],
    )
    .await
    .expect("connect client");
    assert_eq!(handshake.capabilities, ["artifacts"]);

    let meta = std::collections::BTreeMap::from([
        ("workspace_id".to_owned(), serde_json::json!("workspace-1")),
        ("user_id".to_owned(), serde_json::json!("user-7")),
        ("channel_id".to_owned(), serde_json::json!("channel-7")),
    ]);
    let fixture: ArtifactValue = serde_json::from_str(MANAGED_ARTIFACT).expect("artifact fixture");
    let content = b"managed artifact content crosses several chunks";
    let begin = client
        .begin_artifact_upload(&RpcParams {
            meta: meta.clone(),
            data: ArtifactUploadBeginData {
                id: Some("artifact-uploaded".to_owned()),
                media_type: "application/octet-stream".to_owned(),
                name: Some("content.bin".to_owned()),
                expected_byte_length: Some(content.len() as u64),
                expected_digest: None,
                metadata: fixture.metadata,
            },
        })
        .await
        .expect("begin upload");
    let first = &content[..13];
    let first_chunk = client
        .upload_artifact_chunk(&RpcParams {
            meta: meta.clone(),
            data: ArtifactUploadChunkData {
                upload_id: begin.data.upload_id.clone(),
                offset: 0,
                bytes_base64: BASE64.encode(first),
            },
        })
        .await
        .expect("upload first chunk");
    client
        .upload_artifact_chunk(&RpcParams {
            meta: meta.clone(),
            data: ArtifactUploadChunkData {
                upload_id: begin.data.upload_id.clone(),
                offset: first_chunk.data.next_offset,
                bytes_base64: BASE64.encode(&content[first.len()..]),
            },
        })
        .await
        .expect("upload second chunk");
    let committed = client
        .commit_artifact_upload(&RpcParams {
            meta: meta.clone(),
            data: ArtifactUploadCommitData {
                upload_id: begin.data.upload_id,
            },
        })
        .await
        .expect("commit upload");
    let EntityVersion::Active { value, .. } = committed.data.entity else {
        panic!("committed artifact must be active");
    };
    let artifact: ArtifactValue = serde_json::from_value(value).expect("managed artifact value");
    assert_eq!(artifact.byte_length, Some(content.len() as u64));
    assert!(artifact.digest.unwrap().starts_with("sha256:"));

    let mut downloaded = Vec::new();
    let mut offset = 0;
    loop {
        let chunk = client
            .download_artifact_chunk(&RpcParams {
                meta: meta.clone(),
                data: ArtifactDownloadChunkData {
                    id: "artifact-uploaded".to_owned(),
                    version: None,
                    offset,
                    max_bytes: 7,
                },
            })
            .await
            .expect("download artifact chunk");
        downloaded.extend(
            BASE64
                .decode(chunk.data.bytes_base64)
                .expect("decode artifact chunk"),
        );
        offset = chunk.data.next_offset;
        if chunk.data.eof {
            break;
        }
    }
    assert_eq!(downloaded, content);

    let mut wrong_scope = meta;
    wrong_scope.insert("user_id".to_owned(), serde_json::json!("user-8"));
    let error = client
        .download_artifact_chunk(&RpcParams {
            meta: wrong_scope,
            data: ArtifactDownloadChunkData {
                id: "artifact-uploaded".to_owned(),
                version: None,
                offset: 0,
                max_bytes: 7,
            },
        })
        .await
        .expect_err("artifact must remain scoped");
    assert!(matches!(error, IpcError::Rpc { code: -32003, .. }));

    client.shutdown().await.expect("request shutdown");
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
