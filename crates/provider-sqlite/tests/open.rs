use patchouli_provider::Provider;
use patchouli_provider_sqlite::SqliteProvider;

#[tokio::test]
async fn opens_a_database_and_reports_health() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("nested").join("patchouli.db");

    let provider = SqliteProvider::open(&path).await.expect("open SQLite");

    assert_eq!(provider.kind(), "sqlite");
    provider.health_check().await.expect("healthy SQLite");
    assert!(path.exists());
}
