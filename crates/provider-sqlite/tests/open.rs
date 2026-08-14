use patchouli_provider::Provider;
use patchouli_provider_sqlite::SqliteProvider;

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
