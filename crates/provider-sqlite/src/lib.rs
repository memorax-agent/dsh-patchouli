use std::{path::Path, time::Duration};

use async_trait::async_trait;
use patchouli_provider::{Provider, ProviderError};
use thiserror::Error;
use tokio_rusqlite::{Connection, rusqlite};

#[derive(Debug, Error)]
pub enum SqliteProviderError {
    #[error("failed to create SQLite database directory: {0}")]
    Directory(#[from] std::io::Error),
    #[error("failed to open SQLite database: {0}")]
    Open(#[from] rusqlite::Error),
    #[error("failed to initialize SQLite database: {0}")]
    Initialize(#[from] tokio_rusqlite::Error),
}

pub struct SqliteProvider {
    connection: Connection,
}

impl SqliteProvider {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, SqliteProviderError> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            tokio::fs::create_dir_all(parent).await?;
        }

        let connection = Connection::open(path).await?;
        connection
            .call(|connection| {
                connection.busy_timeout(Duration::from_secs(5))?;
                connection.pragma_update(None, "foreign_keys", "ON")?;
                connection.pragma_update(None, "journal_mode", "WAL")?;
                connection.pragma_update(None, "synchronous", "NORMAL")?;
                Ok(())
            })
            .await?;

        Ok(Self { connection })
    }
}

#[async_trait]
impl Provider for SqliteProvider {
    fn kind(&self) -> &'static str {
        "sqlite"
    }

    async fn health_check(&self) -> Result<(), ProviderError> {
        self.connection
            .call(|connection| connection.query_row("SELECT 1", [], |_| Ok(())))
            .await
            .map_err(|error| ProviderError::new(format!("SQLite health check failed: {error}")))
    }
}
