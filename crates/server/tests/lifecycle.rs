use std::sync::Arc;

use async_trait::async_trait;
use patchouli_provider::{Provider, ProviderError};
use patchouli_server::{LocalClient, LocalServer, ServerOptions};

struct HealthyProvider;

#[async_trait]
impl Provider for HealthyProvider {
    fn kind(&self) -> &'static str {
        "test"
    }

    async fn health_check(&self) -> Result<(), ProviderError> {
        Ok(())
    }
}

#[tokio::test]
async fn daemon_accepts_status_and_shutdown_over_local_ipc() {
    let (_directory, endpoint) = test_endpoint();
    let provider = Arc::new(HealthyProvider);
    let server = LocalServer::bind(
        ServerOptions {
            endpoint: endpoint.clone(),
            node_id: "node-test".to_owned(),
            cluster_id: "cluster-test".to_owned(),
        },
        provider,
    )
    .await
    .expect("bind daemon");
    let task = tokio::spawn(server.run());

    let mut client = LocalClient::connect(&endpoint, "test-client", "1.0.0")
        .await
        .expect("connect client");
    let status = client.status().await.expect("read status");
    assert!(status.data.ready);
    assert_eq!(status.data.provider, "test");
    assert_eq!(status.data.pid, std::process::id());
    assert_eq!(status.data.active_connections, 1);

    let stopped = client.shutdown().await.expect("request shutdown");
    assert!(stopped.data.accepted);
    task.await.expect("server task").expect("server shutdown");

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
