use std::sync::Arc;

use patchouli_provider::{
    EntityCommit, EntityCommitOutcome, EntityKey, Provider, ProviderErrorReason, StoredChangeKind,
    StoredEntityVersion, StoredVersionState,
};
use patchouli_provider_remote::{RemoteProvider, remote_provider_router};
use patchouli_provider_sqlite::SqliteProvider;

#[tokio::test]
async fn remote_provider_round_trips_provider_operations() {
    let directory = tempfile::tempdir().unwrap();
    let storage: Arc<dyn Provider> = Arc::new(
        SqliteProvider::open(directory.path().join("remote.db"))
            .await
            .unwrap(),
    );
    let recovery = storage.initialize().await.unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let app = remote_provider_router(
        Arc::clone(&storage),
        "secret".to_owned(),
        recovery,
        shutdown_rx,
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let remote = RemoteProvider::connect(&format!("http://{address}"), "secret".to_owned())
        .await
        .unwrap();
    assert_eq!(remote.kind(), "remote");
    assert_eq!(remote.initialize().await.unwrap(), recovery);
    remote.health_check().await.unwrap();

    let key = EntityKey {
        scope_json: r#"{"workspace_id":"remote"}"#.to_owned(),
        entity_type: "knowledge".to_owned(),
        entity_id: "knowledge-1".to_owned(),
    };
    let commit = EntityCommit {
        key: key.clone(),
        expected_heads: Vec::new(),
        new_versions: vec![StoredEntityVersion {
            version: "v1".to_owned(),
            state: StoredVersionState::Active,
            value_json: Some(r#"{"content":"remote"}"#.to_owned()),
            crdt_fields: Vec::new(),
        }],
        head_versions: vec!["v1".to_owned()],
        change_kind: StoredChangeKind::Created,
        causal_token: "token-1".to_owned(),
        event_meta_json: "{}".to_owned(),
        session_keys: Vec::new(),
        recorded_at_unix_ms: 1,
        deadline_unix_ms: None,
    };
    assert_eq!(
        remote.commit_entity(commit).await.unwrap(),
        EntityCommitOutcome::Committed
    );
    assert_eq!(
        remote
            .read_entity(&key)
            .await
            .unwrap()
            .unwrap()
            .head_versions,
        vec!["v1"]
    );
    let update = EntityCommit {
        key: key.clone(),
        expected_heads: vec!["v1".to_owned()],
        new_versions: vec![StoredEntityVersion {
            version: "v2".to_owned(),
            state: StoredVersionState::Active,
            value_json: Some(r#"{"content":"updated"}"#.to_owned()),
            crdt_fields: Vec::new(),
        }],
        head_versions: vec!["v2".to_owned()],
        change_kind: StoredChangeKind::Updated,
        causal_token: "token-2".to_owned(),
        event_meta_json: "{}".to_owned(),
        session_keys: Vec::new(),
        recorded_at_unix_ms: 2,
        deadline_unix_ms: None,
    };
    let expired = remote
        .commit_entity(EntityCommit {
            deadline_unix_ms: Some(0),
            ..update.clone()
        })
        .await
        .unwrap_err();
    assert_eq!(expired.reason(), ProviderErrorReason::DeadlineExceeded);
    let (waited, committed) = tokio::join!(
        remote.wait_for_changes(&key.scope_json, 1),
        storage.commit_entity(update)
    );
    waited.unwrap();
    assert_eq!(committed.unwrap(), EntityCommitOutcome::Committed);

    let error =
        match RemoteProvider::connect(&format!("http://{address}"), "wrong".to_owned()).await {
            Ok(_) => panic!("wrong token must be rejected"),
            Err(error) => error,
        };
    assert_eq!(error.reason(), ProviderErrorReason::Unauthenticated);
    assert!(error.to_string().contains("unauthenticated"));

    let wait = tokio::spawn(async move { remote.wait_for_changes(&key.scope_json, 2).await });
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    shutdown_tx.send(true).unwrap();
    let error = tokio::time::timeout(std::time::Duration::from_secs(1), wait)
        .await
        .expect("shutdown must release remote wait")
        .unwrap()
        .unwrap_err();
    assert_eq!(error.reason(), ProviderErrorReason::Unavailable);

    server.abort();
    storage.shutdown().await.unwrap();
}

#[tokio::test]
async fn classifies_connection_failures_as_unavailable() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);

    let error =
        match RemoteProvider::connect(&format!("http://{address}"), "secret".to_owned()).await {
            Ok(_) => panic!("closed endpoint must be unavailable"),
            Err(error) => error,
        };
    assert_eq!(error.reason(), ProviderErrorReason::Unavailable);
    assert!(error.to_string().contains("fetch provider info"));
    assert!(error.to_string().contains(&address.to_string()));
}

#[test]
fn rejects_cleartext_non_loopback_endpoints() {
    assert!(RemoteProvider::validate_endpoint("http://example.com").is_err());
    assert!(RemoteProvider::validate_endpoint("https://example.com").is_ok());
}
