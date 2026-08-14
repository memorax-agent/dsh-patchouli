use std::{
    ffi::OsString,
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use fs4::FileExt;
use patchouli_provider::{Provider, ProviderError, ProviderRecovery};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio_rusqlite::{Connection, rusqlite};

const STORAGE_SCHEMA_VERSION: i64 = 2;

#[derive(Debug, Error)]
pub enum SqliteProviderError {
    #[error("SQLite provider filesystem error: {0}")]
    Filesystem(#[from] std::io::Error),
    #[error("failed to acquire exclusive database lock at {path}: {message}")]
    Lock { path: PathBuf, message: String },
    #[error("failed to open SQLite database: {0}")]
    Open(#[from] rusqlite::Error),
    #[error("failed to initialize SQLite database: {0}")]
    Initialize(#[from] tokio_rusqlite::Error),
}

pub struct SqliteProvider {
    connection: Mutex<Option<Connection>>,
    lock: Mutex<Option<File>>,
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

        let lock_path = lock_path(path);
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;
        FileExt::try_lock(&lock).map_err(|error| SqliteProviderError::Lock {
            path: lock_path,
            message: error.to_string(),
        })?;

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

        Ok(Self {
            connection: Mutex::new(Some(connection)),
            lock: Mutex::new(Some(lock)),
        })
    }

    async fn connection(&self) -> Result<Connection, ProviderError> {
        self.connection
            .lock()
            .await
            .as_ref()
            .cloned()
            .ok_or_else(|| ProviderError::new("SQLite provider is shut down"))
    }
}

#[async_trait]
impl Provider for SqliteProvider {
    fn kind(&self) -> &'static str {
        "sqlite"
    }

    async fn initialize(&self) -> Result<ProviderRecovery, ProviderError> {
        let connection = self.connection().await?;
        let started_at_unix_ms = unix_time_ms()?;
        connection
            .call(move |connection| -> Result<ProviderRecovery, ProviderError> {
                let transaction = connection
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                    .map_err(database_error)?;
                let schema_version: i64 = transaction
                    .query_row("PRAGMA user_version", [], |row| row.get(0))
                    .map_err(database_error)?;
                if schema_version == 0 {
                    transaction
                        .pragma_update(None, "user_version", STORAGE_SCHEMA_VERSION)
                        .map_err(database_error)?;
                } else if schema_version != STORAGE_SCHEMA_VERSION {
                    return Err(ProviderError::new(format!(
                        "unsupported SQLite storage schema version {schema_version}; expected {STORAGE_SCHEMA_VERSION}"
                    )));
                }

                transaction
                    .execute_batch(
                        "CREATE TABLE IF NOT EXISTS patchouli_runtime_state (
                            singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                            generation INTEGER NOT NULL CHECK (generation >= 0),
                            running INTEGER NOT NULL CHECK (running IN (0, 1)),
                            started_at_unix_ms INTEGER NOT NULL,
                            clean_shutdown_at_unix_ms INTEGER
                        ) STRICT;
                        INSERT OR IGNORE INTO patchouli_runtime_state (
                            singleton,
                            generation,
                            running,
                            started_at_unix_ms,
                            clean_shutdown_at_unix_ms
                        ) VALUES (1, 0, 0, 0, NULL);

                        CREATE TABLE IF NOT EXISTS patchouli_entity_version (
                            scope_json TEXT NOT NULL
                                CHECK (json_valid(scope_json) AND json_type(scope_json) = 'object'),
                            entity_type TEXT NOT NULL CHECK (length(entity_type) > 0),
                            entity_id TEXT NOT NULL CHECK (length(entity_id) > 0),
                            version TEXT NOT NULL CHECK (length(version) > 0),
                            state TEXT NOT NULL CHECK (state IN ('active', 'deleted')),
                            value_json TEXT,
                            recorded_at_unix_ms INTEGER NOT NULL
                                CHECK (recorded_at_unix_ms >= 0),
                            PRIMARY KEY (scope_json, entity_type, entity_id, version),
                            CHECK (
                                (state = 'active' AND value_json IS NOT NULL AND json_valid(value_json))
                                OR (state = 'deleted' AND value_json IS NULL)
                            )
                        ) STRICT;

                        CREATE TABLE IF NOT EXISTS patchouli_entity_head (
                            scope_json TEXT NOT NULL,
                            entity_type TEXT NOT NULL,
                            entity_id TEXT NOT NULL,
                            version TEXT NOT NULL,
                            PRIMARY KEY (scope_json, entity_type, entity_id, version),
                            FOREIGN KEY (scope_json, entity_type, entity_id, version)
                                REFERENCES patchouli_entity_version (
                                    scope_json,
                                    entity_type,
                                    entity_id,
                                    version
                                ) ON DELETE CASCADE
                        ) STRICT, WITHOUT ROWID;

                        CREATE VIEW IF NOT EXISTS patchouli_knowledge AS
                        SELECT
                            version.scope_json,
                            version.entity_id AS knowledge_id,
                            version.version,
                            json_extract(version.value_json, '$.content') AS content_json,
                            json_extract(version.value_json, '$.metadata') AS metadata_json,
                            json_extract(version.value_json, '$.artifact') AS artifact_json,
                            json_extract(version.value_json, '$.profile') AS profile_json,
                            version.recorded_at_unix_ms
                        FROM patchouli_entity_version AS version
                        INNER JOIN patchouli_entity_head AS head USING (
                            scope_json,
                            entity_type,
                            entity_id,
                            version
                        )
                        WHERE version.entity_type = 'knowledge' AND version.state = 'active';

                        CREATE VIEW IF NOT EXISTS patchouli_knowledge_relation AS
                        SELECT
                            version.scope_json,
                            version.entity_id AS relation_id,
                            version.version,
                            json_extract(version.value_json, '$.type') AS relation_type,
                            json_extract(version.value_json, '$.from') AS from_knowledge_refs_json,
                            json_extract(version.value_json, '$.to') AS to_knowledge_refs_json,
                            json_extract(version.value_json, '$.metadata') AS metadata_json,
                            version.recorded_at_unix_ms
                        FROM patchouli_entity_version AS version
                        INNER JOIN patchouli_entity_head AS head USING (
                            scope_json,
                            entity_type,
                            entity_id,
                            version
                        )
                        WHERE version.entity_type = 'knowledge_relation'
                            AND version.state = 'active';",
                    )
                    .map_err(database_error)?;

                let (generation, running): (u64, bool) = transaction
                    .query_row(
                        "SELECT generation, running != 0
                         FROM patchouli_runtime_state
                         WHERE singleton = 1",
                        [],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .map_err(database_error)?;
                let generation = generation
                    .checked_add(1)
                    .ok_or_else(|| ProviderError::new("runtime generation overflow"))?;
                transaction
                    .execute(
                        "UPDATE patchouli_runtime_state
                         SET generation = ?1,
                             running = 1,
                             started_at_unix_ms = ?2,
                             clean_shutdown_at_unix_ms = NULL
                         WHERE singleton = 1",
                        (generation, started_at_unix_ms),
                    )
                    .map_err(database_error)?;
                transaction.commit().map_err(database_error)?;

                Ok(ProviderRecovery {
                    generation,
                    recovered_after_unclean_shutdown: running,
                })
            })
            .await
            .map_err(|error| ProviderError::new(format!("SQLite initialization failed: {error}")))
    }

    async fn health_check(&self) -> Result<(), ProviderError> {
        let connection = self.connection().await?;
        connection
            .call(|connection| connection.query_row("SELECT 1", [], |_| Ok(())))
            .await
            .map_err(|error| ProviderError::new(format!("SQLite health check failed: {error}")))
    }

    async fn checkpoint(&self) -> Result<(), ProviderError> {
        let connection = self.connection().await?;
        connection
            .call(|connection| -> Result<(), ProviderError> {
                let (busy, log_frames, checkpointed_frames): (u64, u64, u64) = connection
                    .query_row("PRAGMA wal_checkpoint(FULL)", [], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                    })
                    .map_err(database_error)?;
                if busy != 0 || checkpointed_frames != log_frames {
                    return Err(ProviderError::new(format!(
                        "SQLite checkpoint was incomplete: busy={busy}, log_frames={log_frames}, checkpointed_frames={checkpointed_frames}"
                    )));
                }
                Ok(())
            })
            .await
            .map_err(|error| ProviderError::new(format!("SQLite checkpoint failed: {error}")))
    }

    async fn shutdown(&self) -> Result<(), ProviderError> {
        let Some(connection) = self.connection.lock().await.take() else {
            return Ok(());
        };
        let clean_shutdown_at_unix_ms = unix_time_ms()?;
        let persist_result = connection
            .call(move |connection| -> Result<(), ProviderError> {
                let transaction = connection
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                    .map_err(database_error)?;
                transaction
                    .execute(
                        "UPDATE patchouli_runtime_state
                         SET running = 0, clean_shutdown_at_unix_ms = ?1
                         WHERE singleton = 1",
                        [clean_shutdown_at_unix_ms],
                    )
                    .map_err(database_error)?;
                transaction.commit().map_err(database_error)?;
                let busy: u64 = connection
                    .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))
                    .map_err(database_error)?;
                if busy != 0 {
                    return Err(ProviderError::new(
                        "SQLite WAL could not be truncated during shutdown",
                    ));
                }
                Ok(())
            })
            .await
            .map_err(|error| ProviderError::new(format!("SQLite shutdown failed: {error}")));
        let close_result = connection
            .close()
            .await
            .map_err(|error| ProviderError::new(format!("SQLite close failed: {error}")));
        drop(self.lock.lock().await.take());
        persist_result?;
        close_result
    }
}

fn lock_path(path: &Path) -> PathBuf {
    let mut value = OsString::from(path.as_os_str());
    value.push(".lock");
    PathBuf::from(value)
}

fn unix_time_ms() -> Result<u64, ProviderError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .map_err(|error| ProviderError::new(format!("system clock is before Unix epoch: {error}")))
}

fn database_error(error: rusqlite::Error) -> ProviderError {
    ProviderError::new(format!("SQLite database error: {error}"))
}
