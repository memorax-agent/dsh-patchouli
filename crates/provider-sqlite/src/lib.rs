use std::{
    ffi::OsString,
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};

use async_trait::async_trait;
use fs4::FileExt;
use patchouli_provider::{
    ChangePage, ChangeQuery, ConsistencyAcquireOutcome, ConsistencyQuery, EntityCommit,
    EntityCommitOutcome, EntityKey, EntitySnapshot, IdempotencyReadOutcome, IdempotencyRecord,
    IdempotentCommitOutcome, Provider, ProviderCapabilities, ProviderError, ProviderRecovery,
    RetrieveQuery, RetrievedEntity, StoredChange, StoredChangeKind, StoredCrdtChange,
    StoredCrdtField, StoredEntityVersion, StoredVersionState, WorkUnit, WorkUnitCommit,
    WorkUnitCommitOutcome, WorkUnitConflict, WorkUnitExpiryAction, WorkUnitPublish,
    WorkUnitReadOutcome,
};
use thiserror::Error;
use tokio::sync::{Mutex, watch};
use tokio_rusqlite::rusqlite::OptionalExtension;
use tokio_rusqlite::{Connection, rusqlite};

const STORAGE_SCHEMA_VERSION: i64 = 11;

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
    changes: watch::Sender<u64>,
}

impl SqliteProvider {
    pub fn validate_existing_storage(path: impl AsRef<Path>) -> Result<(), SqliteProviderError> {
        let Some(path) = canonical_existing_storage_path(path.as_ref())? else {
            return Ok(());
        };
        validate_existing_private_file(&path)?;
        validate_existing_private_file(&lock_path(&path))?;
        validate_private_sidecars(&path)?;
        Ok(())
    }

    pub async fn open(path: impl AsRef<Path>) -> Result<Self, SqliteProviderError> {
        let requested_path = path.as_ref();
        let parent = storage_parent(requested_path);
        reject_symbolic_link_components(parent)?;
        if parent != Path::new(".") {
            create_private_dir(parent)?;
        }
        let path = canonical_storage_path(requested_path)?;
        Self::validate_existing_storage(&path)?;

        let database_file = open_private_file(&path)?;
        drop(database_file);

        let lock_path = lock_path(&path);
        let lock = open_private_file(&lock_path)?;
        FileExt::try_lock(&lock).map_err(|error| SqliteProviderError::Lock {
            path: lock_path,
            message: error.to_string(),
        })?;

        let connection = Connection::open(&path).await?;
        connection
            .call(|connection| {
                connection.busy_timeout(Duration::from_secs(5))?;
                connection.pragma_update(None, "foreign_keys", "ON")?;
                connection.pragma_update(None, "journal_mode", "WAL")?;
                connection.pragma_update(None, "synchronous", "NORMAL")?;
                Ok(())
            })
            .await?;
        validate_private_sidecars(&path)?;

        let (changes, _) = watch::channel(0);
        Ok(Self {
            connection: Mutex::new(Some(connection)),
            lock: Mutex::new(Some(lock)),
            changes,
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

    fn signal_changes(&self) {
        self.changes.send_modify(|generation| *generation += 1);
    }
}

fn canonical_existing_storage_path(path: &Path) -> Result<Option<PathBuf>, std::io::Error> {
    let parent = storage_parent(path);
    match std::fs::metadata(parent) {
        Ok(metadata) if metadata.is_dir() => canonical_storage_path(path).map(Some),
        Ok(_) => Err(std::io::Error::new(
            std::io::ErrorKind::NotADirectory,
            format!("{} is not a directory", parent.display()),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn canonical_storage_path(path: &Path) -> Result<PathBuf, std::io::Error> {
    let file_name = path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} does not name a database file", path.display()),
        )
    })?;
    let parent = storage_parent(path);
    reject_symbolic_link_components(parent)?;
    let parent = std::fs::canonicalize(parent)?;
    validate_storage_owner(&parent)?;
    Ok(parent.join(file_name))
}

fn storage_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn reject_symbolic_link_components(path: &Path) -> Result<(), std::io::Error> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    for component in absolute.ancestors().collect::<Vec<_>>().into_iter().rev() {
        match std::fs::symlink_metadata(component) {
            Ok(metadata)
                if metadata.file_type().is_symlink()
                    && !symbolic_link_owner_is_trusted(&metadata) =>
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!(
                        "storage path component {} is an untrusted symbolic link",
                        component.display()
                    ),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn symbolic_link_owner_is_trusted(metadata: &std::fs::Metadata) -> bool {
    let owner = metadata.uid();
    // SAFETY: geteuid has no preconditions and does not retain pointers.
    owner == 0 || owner == unsafe { libc::geteuid() }
}

#[cfg(not(unix))]
fn symbolic_link_owner_is_trusted(_metadata: &std::fs::Metadata) -> bool {
    false
}

fn create_private_dir(path: &Path) -> Result<(), std::io::Error> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => return Ok(()),
        Ok(_) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("{} exists and is not a directory", path.display()),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(path)
}

fn open_private_file(path: &Path) -> Result<File, std::io::Error> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    let file = options.open(path)?;
    validate_private_file_permissions(path, &file)?;
    Ok(file)
}

fn validate_existing_private_file(path: &Path) -> Result<(), std::io::Error> {
    #[cfg(not(unix))]
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("{} must not be a symbolic link", path.display()),
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW);
    match options.open(path) {
        Ok(file) => validate_private_file_permissions(path, &file),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn validate_storage_owner(path: &Path) -> Result<(), std::io::Error> {
    let metadata = std::fs::metadata(path)?;
    let mode = metadata.mode();
    // SAFETY: geteuid has no preconditions and does not retain pointers.
    let effective_user = unsafe { libc::geteuid() };
    if metadata.uid() != effective_user {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("{} is not owned by the current user", path.display()),
        ));
    }
    if mode & 0o022 != 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "{} is writable by other users (mode {:03o})",
                path.display(),
                mode & 0o777
            ),
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_storage_owner(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

#[cfg(unix)]
fn validate_private_sidecars(path: &Path) -> Result<(), std::io::Error> {
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = OsString::from(path.as_os_str());
        sidecar.push(suffix);
        let sidecar = PathBuf::from(sidecar);
        validate_existing_private_file(&sidecar)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_private_sidecars(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

#[cfg(unix)]
fn validate_private_file_permissions(path: &Path, file: &File) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;

    validate_mode(path, file.metadata()?.permissions().mode())
}

#[cfg(unix)]
fn validate_mode(path: &Path, mode: u32) -> Result<(), std::io::Error> {
    if mode & 0o077 == 0 {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "{} is accessible by other users (mode {:03o}); expected no group/other permissions",
                path.display(),
                mode & 0o777
            ),
        ))
    }
}

#[cfg(not(unix))]
fn validate_private_file_permissions(_path: &Path, _file: &File) -> Result<(), std::io::Error> {
    Ok(())
}

#[async_trait]
impl Provider for SqliteProvider {
    fn kind(&self) -> &'static str {
        "sqlite"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            authority: true,
            replica: false,
            change_stream: true,
            retrieval: true,
            idempotency: true,
            work_units: true,
            causal_sessions: true,
        }
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

                        CREATE TABLE IF NOT EXISTS patchouli_work_unit (
                            identity_json TEXT PRIMARY KEY
                                CHECK (
                                    json_valid(identity_json)
                                    AND json_type(identity_json) = 'object'
                                ),
                            scope_json TEXT NOT NULL
                                CHECK (json_valid(scope_json) AND json_type(scope_json) = 'object'),
                            state TEXT NOT NULL
                                CHECK (state IN ('open', 'closing', 'committed', 'expired')),
                            policy_json TEXT NOT NULL CHECK (json_valid(policy_json)),
                            expiry_action TEXT NOT NULL CHECK (expiry_action = 'discard'),
                            opened_at_unix_ms INTEGER NOT NULL
                                CHECK (opened_at_unix_ms >= 0),
                            expires_at_unix_ms INTEGER NOT NULL
                                CHECK (expires_at_unix_ms > opened_at_unix_ms),
                            baseline_cursor INTEGER NOT NULL CHECK (baseline_cursor >= 0),
                            closed_at_unix_ms INTEGER,
                            CHECK (
                                (state IN ('open', 'closing') AND closed_at_unix_ms IS NULL)
                                OR (state IN ('committed', 'expired') AND closed_at_unix_ms IS NOT NULL)
                            )
                        ) STRICT, WITHOUT ROWID;

                        CREATE TABLE IF NOT EXISTS patchouli_entity_version (
                            scope_json TEXT NOT NULL
                                CHECK (json_valid(scope_json) AND json_type(scope_json) = 'object'),
                            entity_type TEXT NOT NULL CHECK (length(entity_type) > 0),
                            entity_id TEXT NOT NULL CHECK (length(entity_id) > 0),
                            version TEXT NOT NULL CHECK (length(version) > 0),
                            state TEXT NOT NULL CHECK (state IN ('active', 'deleted')),
                            value_json TEXT,
                            work_unit_json TEXT,
                            published_cursor INTEGER CHECK (published_cursor >= 1),
                            recorded_at_unix_ms INTEGER NOT NULL
                                CHECK (recorded_at_unix_ms >= 0),
                            PRIMARY KEY (scope_json, entity_type, entity_id, version),
                            FOREIGN KEY (work_unit_json)
                                REFERENCES patchouli_work_unit (identity_json) ON DELETE CASCADE,
                            CHECK (
                                (state = 'active' AND value_json IS NOT NULL AND json_valid(value_json))
                                OR (state = 'deleted' AND value_json IS NULL)
                            )
                        ) STRICT;

                        CREATE TABLE IF NOT EXISTS patchouli_work_unit_entity (
                            work_unit_json TEXT NOT NULL,
                            scope_json TEXT NOT NULL,
                            entity_type TEXT NOT NULL,
                            entity_id TEXT NOT NULL,
                            conflict_policy_json TEXT CHECK (json_valid(conflict_policy_json)),
                            causal_token TEXT,
                            event_meta_json TEXT CHECK (json_valid(event_meta_json)),
                            session_keys_json TEXT CHECK (json_valid(session_keys_json)),
                            close_marker INTEGER NOT NULL DEFAULT 0 CHECK (close_marker IN (0, 1)),
                            PRIMARY KEY (
                                work_unit_json,
                                scope_json,
                                entity_type,
                                entity_id
                            ),
                            FOREIGN KEY (work_unit_json)
                                REFERENCES patchouli_work_unit (identity_json) ON DELETE CASCADE
                        ) STRICT, WITHOUT ROWID;

                        CREATE TABLE IF NOT EXISTS patchouli_work_unit_base_version (
                            work_unit_json TEXT NOT NULL,
                            scope_json TEXT NOT NULL,
                            entity_type TEXT NOT NULL,
                            entity_id TEXT NOT NULL,
                            version TEXT NOT NULL,
                            is_head INTEGER NOT NULL CHECK (is_head IN (0, 1)),
                            PRIMARY KEY (
                                work_unit_json,
                                scope_json,
                                entity_type,
                                entity_id,
                                version
                            ),
                            FOREIGN KEY (
                                work_unit_json,
                                scope_json,
                                entity_type,
                                entity_id
                            ) REFERENCES patchouli_work_unit_entity (
                                work_unit_json,
                                scope_json,
                                entity_type,
                                entity_id
                            ) ON DELETE CASCADE,
                            FOREIGN KEY (scope_json, entity_type, entity_id, version)
                                REFERENCES patchouli_entity_version (
                                    scope_json,
                                    entity_type,
                                    entity_id,
                                    version
                                ) ON DELETE RESTRICT
                        ) STRICT, WITHOUT ROWID;

                        CREATE TABLE IF NOT EXISTS patchouli_work_unit_head (
                            work_unit_json TEXT NOT NULL,
                            scope_json TEXT NOT NULL,
                            entity_type TEXT NOT NULL,
                            entity_id TEXT NOT NULL,
                            version TEXT NOT NULL,
                            PRIMARY KEY (
                                work_unit_json,
                                scope_json,
                                entity_type,
                                entity_id,
                                version
                            ),
                            FOREIGN KEY (
                                work_unit_json,
                                scope_json,
                                entity_type,
                                entity_id
                            ) REFERENCES patchouli_work_unit_entity (
                                work_unit_json,
                                scope_json,
                                entity_type,
                                entity_id
                            ) ON DELETE CASCADE,
                            FOREIGN KEY (scope_json, entity_type, entity_id, version)
                                REFERENCES patchouli_entity_version (
                                    scope_json,
                                    entity_type,
                                    entity_id,
                                    version
                                ) ON DELETE CASCADE
                        ) STRICT, WITHOUT ROWID;

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

                        CREATE TABLE IF NOT EXISTS patchouli_entity_head_history (
                            scope_json TEXT NOT NULL,
                            entity_type TEXT NOT NULL,
                            entity_id TEXT NOT NULL,
                            cursor INTEGER NOT NULL CHECK (cursor > 0),
                            head_versions_json TEXT NOT NULL CHECK (
                                json_valid(head_versions_json)
                                AND json_type(head_versions_json) = 'array'
                                AND json_array_length(head_versions_json) > 0
                            ),
                            PRIMARY KEY (scope_json, entity_type, entity_id, cursor)
                        ) STRICT, WITHOUT ROWID;

                        CREATE TABLE IF NOT EXISTS patchouli_crdt_change (
                            change_hash TEXT PRIMARY KEY CHECK (length(change_hash) > 0),
                            change_bytes BLOB NOT NULL CHECK (length(change_bytes) > 0)
                        ) STRICT, WITHOUT ROWID;

                        CREATE TABLE IF NOT EXISTS patchouli_crdt_change_parent (
                            change_hash TEXT NOT NULL,
                            parent_hash TEXT NOT NULL,
                            PRIMARY KEY (change_hash, parent_hash),
                            FOREIGN KEY (change_hash)
                                REFERENCES patchouli_crdt_change (change_hash) ON DELETE CASCADE,
                            FOREIGN KEY (parent_hash)
                                REFERENCES patchouli_crdt_change (change_hash) ON DELETE CASCADE,
                            CHECK (change_hash != parent_hash)
                        ) STRICT, WITHOUT ROWID;

                        CREATE TABLE IF NOT EXISTS patchouli_entity_crdt_head (
                            scope_json TEXT NOT NULL,
                            entity_type TEXT NOT NULL,
                            entity_id TEXT NOT NULL,
                            version TEXT NOT NULL,
                            field_path TEXT NOT NULL
                                CHECK (length(field_path) > 1 AND substr(field_path, 1, 1) = '/'),
                            change_hash TEXT NOT NULL,
                            PRIMARY KEY (
                                scope_json,
                                entity_type,
                                entity_id,
                                version,
                                field_path,
                                change_hash
                            ),
                            FOREIGN KEY (scope_json, entity_type, entity_id, version)
                                REFERENCES patchouli_entity_version (
                                    scope_json,
                                    entity_type,
                                    entity_id,
                                    version
                                ) ON DELETE CASCADE,
                            FOREIGN KEY (change_hash)
                                REFERENCES patchouli_crdt_change (change_hash) ON DELETE RESTRICT
                        ) STRICT, WITHOUT ROWID;

                        CREATE TABLE IF NOT EXISTS patchouli_change (
                            cursor INTEGER PRIMARY KEY AUTOINCREMENT,
                            scope_json TEXT NOT NULL
                                CHECK (json_valid(scope_json) AND json_type(scope_json) = 'object'),
                            entity_type TEXT NOT NULL CHECK (length(entity_type) > 0),
                            entity_id TEXT NOT NULL CHECK (length(entity_id) > 0),
                            kind TEXT NOT NULL CHECK (
                                kind IN ('conflicted', 'created', 'deleted', 'resolved', 'updated')
                            ),
                            head_versions_json TEXT NOT NULL CHECK (
                                json_valid(head_versions_json)
                                AND json_type(head_versions_json) = 'array'
                                AND json_array_length(head_versions_json) > 0
                            ),
                            causal_token TEXT NOT NULL UNIQUE CHECK (length(causal_token) > 0),
                            event_meta_json TEXT NOT NULL CHECK (
                                json_valid(event_meta_json)
                                AND json_type(event_meta_json) = 'object'
                            ),
                            recorded_at_unix_ms INTEGER NOT NULL
                                CHECK (recorded_at_unix_ms >= 0)
                        ) STRICT;

                        CREATE TABLE IF NOT EXISTS patchouli_change_retention (
                            scope_json TEXT PRIMARY KEY,
                            pruned_through_cursor INTEGER NOT NULL CHECK (pruned_through_cursor > 0)
                        ) STRICT, WITHOUT ROWID;

                        CREATE TABLE IF NOT EXISTS patchouli_session_frontier (
                            scope_json TEXT NOT NULL CHECK (
                                json_valid(scope_json) AND json_type(scope_json) = 'object'
                            ),
                            session_key_json TEXT NOT NULL CHECK (json_valid(session_key_json)),
                            cursor INTEGER NOT NULL CHECK (cursor >= 0),
                            PRIMARY KEY (scope_json, session_key_json)
                        ) STRICT, WITHOUT ROWID;

                        CREATE TABLE IF NOT EXISTS patchouli_causal_frontier (
                            causal_token TEXT PRIMARY KEY CHECK (length(causal_token) > 0),
                            scope_json TEXT NOT NULL CHECK (
                                json_valid(scope_json) AND json_type(scope_json) = 'object'
                            ),
                            cursor INTEGER NOT NULL CHECK (cursor > 0)
                        ) STRICT, WITHOUT ROWID;

                        CREATE TABLE IF NOT EXISTS patchouli_scope_frontier (
                            scope_json TEXT PRIMARY KEY CHECK (
                                json_valid(scope_json) AND json_type(scope_json) = 'object'
                            ),
                            cursor INTEGER NOT NULL CHECK (cursor > 0),
                            causal_token TEXT NOT NULL,
                            FOREIGN KEY (causal_token)
                                REFERENCES patchouli_causal_frontier (causal_token)
                                ON DELETE RESTRICT
                        ) STRICT, WITHOUT ROWID;

                        CREATE TABLE IF NOT EXISTS patchouli_idempotency (
                            identity_json TEXT PRIMARY KEY CHECK (json_valid(identity_json)),
                            request_json TEXT NOT NULL CHECK (json_valid(request_json)),
                            result_json TEXT NOT NULL CHECK (json_valid(result_json)),
                            expires_at_unix_ms INTEGER NOT NULL CHECK (expires_at_unix_ms >= 0)
                        ) STRICT, WITHOUT ROWID;

                        CREATE TABLE IF NOT EXISTS patchouli_work_unit_idempotency (
                            identity_json TEXT PRIMARY KEY CHECK (json_valid(identity_json)),
                            work_unit_json TEXT NOT NULL,
                            request_json TEXT NOT NULL CHECK (json_valid(request_json)),
                            result_json TEXT NOT NULL CHECK (json_valid(result_json)),
                            expires_at_unix_ms INTEGER NOT NULL CHECK (expires_at_unix_ms >= 0),
                            FOREIGN KEY (work_unit_json)
                                REFERENCES patchouli_work_unit (identity_json) ON DELETE CASCADE
                        ) STRICT, WITHOUT ROWID;

                        CREATE VIEW IF NOT EXISTS patchouli_artifact AS
                        SELECT
                            version.scope_json,
                            version.entity_id AS artifact_id,
                            version.version,
                            json_extract(version.value_json, '$.media_type') AS media_type,
                            json_extract(version.value_json, '$.name') AS name,
                            json_extract(version.value_json, '$.byte_length') AS byte_length,
                            json_extract(version.value_json, '$.digest') AS digest,
                            json_extract(version.value_json, '$.placement.kind') AS placement_kind,
                            json_extract(version.value_json, '$.placement') AS placement_json,
                            json_extract(version.value_json, '$.metadata') AS metadata_json,
                            version.recorded_at_unix_ms
                        FROM patchouli_entity_version AS version
                        INNER JOIN patchouli_entity_head AS head USING (
                            scope_json,
                            entity_type,
                            entity_id,
                            version
                        )
                        WHERE version.entity_type = 'artifact' AND version.state = 'active';

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

                expire_open_work_units(&transaction, started_at_unix_ms)?;

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

    async fn read_entity(&self, key: &EntityKey) -> Result<Option<EntitySnapshot>, ProviderError> {
        let connection = self.connection().await?;
        let key = key.clone();
        connection
            .call(move |connection| {
                sweep_expired_work_units(connection)?;
                read_entity_snapshot(connection, &key)
            })
            .await
            .map_err(|error| ProviderError::new(format!("SQLite entity read failed: {error}")))
    }

    async fn acquire_consistency(
        &self,
        query: ConsistencyQuery,
    ) -> Result<ConsistencyAcquireOutcome, ProviderError> {
        let connection = self.connection().await?;
        connection
            .call(move |connection| acquire_consistency(connection, &query))
            .await
            .map_err(|error| {
                ProviderError::new(format!("SQLite consistency acquisition failed: {error}"))
            })
    }

    async fn read_changes(&self, query: ChangeQuery) -> Result<ChangePage, ProviderError> {
        let connection = self.connection().await?;
        connection
            .call(move |connection| read_changes(connection, &query))
            .await
            .map_err(|error| ProviderError::new(format!("SQLite change read failed: {error}")))
    }

    async fn wait_for_changes(
        &self,
        scope_json: &str,
        after_cursor: u64,
    ) -> Result<(), ProviderError> {
        let mut changes = self.changes.subscribe();
        let scope_json = scope_json.to_owned();
        loop {
            let connection = self.connection().await?;
            let scope = scope_json.clone();
            let current = connection
                .call(move |connection| -> Result<u64, ProviderError> {
                    connection
                        .query_row(
                            "SELECT cursor FROM patchouli_scope_frontier WHERE scope_json = ?1",
                            [scope],
                            |row| row.get(0),
                        )
                        .optional()
                        .map(|cursor| cursor.unwrap_or(0))
                        .map_err(database_error)
                })
                .await
                .map_err(|error| {
                    ProviderError::new(format!("SQLite change wait failed: {error}"))
                })?;
            if current > after_cursor {
                return Ok(());
            }
            changes
                .changed()
                .await
                .map_err(|_| ProviderError::new("SQLite change notifications are unavailable"))?;
        }
    }

    async fn retrieve_entities(
        &self,
        query: RetrieveQuery,
    ) -> Result<Vec<RetrievedEntity>, ProviderError> {
        let connection = self.connection().await?;
        connection
            .call(move |connection| retrieve_entities(connection, &query))
            .await
            .map_err(|error| ProviderError::new(format!("SQLite retrieval failed: {error}")))
    }

    async fn commit_entity(
        &self,
        commit: EntityCommit,
    ) -> Result<EntityCommitOutcome, ProviderError> {
        let connection = self.connection().await?;
        let outcome = connection
            .call(move |connection| {
                sweep_expired_work_units(connection)?;
                commit_entity_transaction(connection, commit)
            })
            .await
            .map_err(|error| sqlite_call_error("entity commit", error))?;
        if matches!(outcome, EntityCommitOutcome::Committed) {
            self.signal_changes();
        }
        Ok(outcome)
    }

    async fn read_idempotency(
        &self,
        identity_json: &str,
        request_json: &str,
        now_unix_ms: u64,
    ) -> Result<IdempotencyReadOutcome, ProviderError> {
        let connection = self.connection().await?;
        let identity_json = identity_json.to_owned();
        let request_json = request_json.to_owned();
        connection
            .call(move |connection| {
                read_idempotency(connection, &identity_json, &request_json, now_unix_ms)
            })
            .await
            .map_err(|error| ProviderError::new(format!("SQLite idempotency read failed: {error}")))
    }

    async fn read_idempotency_in_work_unit(
        &self,
        work_unit: &WorkUnit,
        identity_json: &str,
        request_json: &str,
        now_unix_ms: u64,
        allow_replay: bool,
    ) -> Result<IdempotencyReadOutcome, ProviderError> {
        let connection = self.connection().await?;
        let work_unit = work_unit.clone();
        let identity_json = identity_json.to_owned();
        let request_json = request_json.to_owned();
        connection
            .call(
                move |connection| -> Result<IdempotencyReadOutcome, ProviderError> {
                    let transaction = connection
                        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                        .map_err(database_error)?;
                    let outcome = read_work_unit_idempotency(
                        &transaction,
                        &work_unit,
                        &identity_json,
                        &request_json,
                        now_unix_ms,
                        allow_replay,
                    )?;
                    commit_before_deadline(transaction, work_unit.deadline_unix_ms)?;
                    Ok(outcome)
                },
            )
            .await
            .map_err(|error| sqlite_call_error("work-unit idempotency read", error))
    }

    async fn commit_entity_idempotent(
        &self,
        commit: EntityCommit,
        idempotency: IdempotencyRecord,
        now_unix_ms: u64,
    ) -> Result<IdempotentCommitOutcome, ProviderError> {
        let connection = self.connection().await?;
        let outcome = connection
            .call(move |connection| {
                commit_entity_idempotent(connection, commit, idempotency, now_unix_ms)
            })
            .await
            .map_err(|error| sqlite_call_error("idempotent commit", error))?;
        if matches!(outcome, IdempotentCommitOutcome::Committed) {
            self.signal_changes();
        }
        Ok(outcome)
    }

    async fn read_entity_in_work_unit(
        &self,
        work_unit: &WorkUnit,
        key: &EntityKey,
    ) -> Result<WorkUnitReadOutcome, ProviderError> {
        let connection = self.connection().await?;
        let work_unit = work_unit.clone();
        let key = key.clone();
        connection
            .call(move |connection| {
                sweep_expired_work_units(connection)?;
                read_entity_in_work_unit(connection, &work_unit, &key)
            })
            .await
            .map_err(|error| sqlite_call_error("work-unit read", error))
    }

    async fn commit_entity_in_work_unit(
        &self,
        commit: WorkUnitCommit,
    ) -> Result<WorkUnitCommitOutcome, ProviderError> {
        let connection = self.connection().await?;
        let outcome = connection
            .call(move |connection| {
                sweep_expired_work_units(connection)?;
                commit_entity_in_work_unit(connection, commit)
            })
            .await
            .map_err(|error| sqlite_call_error("work-unit commit", error))?;
        if matches!(outcome, WorkUnitCommitOutcome::Published) {
            self.signal_changes();
        }
        Ok(outcome)
    }

    async fn publish_work_unit(
        &self,
        publish: WorkUnitPublish,
    ) -> Result<WorkUnitCommitOutcome, ProviderError> {
        let connection = self.connection().await?;
        let outcome = connection
            .call(move |connection| {
                sweep_expired_work_units(connection)?;
                publish_work_unit(connection, publish)
            })
            .await
            .map_err(|error| {
                ProviderError::new(format!("SQLite work-unit publication failed: {error}"))
            })?;
        if matches!(outcome, WorkUnitCommitOutcome::Published) {
            self.signal_changes();
        }
        Ok(outcome)
    }

    async fn checkpoint(&self) -> Result<(), ProviderError> {
        let connection = self.connection().await?;
        connection
            .call(|connection| -> Result<(), ProviderError> {
                sweep_expired_work_units(connection)?;
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
                expire_open_work_units(&transaction, clean_shutdown_at_unix_ms)?;
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

fn commit_before_deadline(
    transaction: rusqlite::Transaction<'_>,
    deadline_unix_ms: Option<u64>,
) -> Result<(), ProviderError> {
    if let Some(deadline) = deadline_unix_ms
        && unix_time_ms()? >= deadline
    {
        return Err(ProviderError::deadline_exceeded());
    }
    transaction.commit().map_err(database_error)
}

fn database_error(error: rusqlite::Error) -> ProviderError {
    ProviderError::new(format!("SQLite database error: {error}"))
}

fn sqlite_call_error(
    operation: &str,
    error: tokio_rusqlite::Error<ProviderError>,
) -> ProviderError {
    match error {
        tokio_rusqlite::Error::Error(error) => error,
        error => ProviderError::new(format!("SQLite {operation} failed: {error}")),
    }
}

fn read_idempotency(
    connection: &mut rusqlite::Connection,
    identity_json: &str,
    request_json: &str,
    now_unix_ms: u64,
) -> Result<IdempotencyReadOutcome, ProviderError> {
    connection
        .execute(
            "DELETE FROM patchouli_idempotency WHERE expires_at_unix_ms <= ?1",
            [now_unix_ms],
        )
        .map_err(database_error)?;
    connection
        .execute(
            "DELETE FROM patchouli_work_unit_idempotency WHERE expires_at_unix_ms <= ?1",
            [now_unix_ms],
        )
        .map_err(database_error)?;
    let stored = connection
        .query_row(
            "SELECT request_json, result_json
             FROM patchouli_idempotency
             WHERE identity_json = ?1",
            [identity_json],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(database_error)?;
    let outcome = match stored {
        None => IdempotencyReadOutcome::Missing,
        Some((stored_request, result_json)) if stored_request == request_json => {
            IdempotencyReadOutcome::Replayed { result_json }
        }
        Some(_) => IdempotencyReadOutcome::Conflict,
    };
    if outcome != IdempotencyReadOutcome::Missing {
        return Ok(outcome);
    }
    let staged = connection
        .query_row(
            "SELECT 1 FROM patchouli_work_unit_idempotency WHERE identity_json = ?1",
            [identity_json],
            |_| Ok(()),
        )
        .optional()
        .map_err(database_error)?;
    Ok(if staged.is_some() {
        IdempotencyReadOutcome::Conflict
    } else {
        IdempotencyReadOutcome::Missing
    })
}

fn read_work_unit_idempotency(
    transaction: &rusqlite::Transaction<'_>,
    work_unit: &WorkUnit,
    identity_json: &str,
    request_json: &str,
    now_unix_ms: u64,
    allow_replay: bool,
) -> Result<IdempotencyReadOutcome, ProviderError> {
    match ensure_work_unit(transaction, work_unit)? {
        StoredWorkUnitState::PolicyMismatch => return Ok(IdempotencyReadOutcome::Conflict),
        StoredWorkUnitState::Expired => return Ok(IdempotencyReadOutcome::Missing),
        StoredWorkUnitState::Open
        | StoredWorkUnitState::Closing
        | StoredWorkUnitState::Committed => {}
    }
    transaction
        .execute(
            "DELETE FROM patchouli_idempotency WHERE expires_at_unix_ms <= ?1",
            [now_unix_ms],
        )
        .map_err(database_error)?;
    transaction
        .execute(
            "DELETE FROM patchouli_work_unit_idempotency WHERE expires_at_unix_ms <= ?1",
            [now_unix_ms],
        )
        .map_err(database_error)?;
    if let Some((stored_request, result_json)) = transaction
        .query_row(
            "SELECT request_json, result_json
             FROM patchouli_idempotency WHERE identity_json = ?1",
            [identity_json],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(database_error)?
    {
        return Ok(if stored_request == request_json {
            IdempotencyReadOutcome::Replayed { result_json }
        } else {
            IdempotencyReadOutcome::Conflict
        });
    }
    let staged = transaction
        .query_row(
            "SELECT work_unit_json, request_json, result_json
             FROM patchouli_work_unit_idempotency WHERE identity_json = ?1",
            [identity_json],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(database_error)?;
    Ok(match staged {
        None => IdempotencyReadOutcome::Missing,
        Some((unit, stored_request, result_json))
            if unit == work_unit.identity_json && stored_request == request_json =>
        {
            if allow_replay {
                IdempotencyReadOutcome::Replayed { result_json }
            } else {
                IdempotencyReadOutcome::Missing
            }
        }
        Some(_) => IdempotencyReadOutcome::Conflict,
    })
}

fn acquire_consistency(
    connection: &mut rusqlite::Connection,
    query: &ConsistencyQuery,
) -> Result<ConsistencyAcquireOutcome, ProviderError> {
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(database_error)?;
    let current = transaction
        .query_row(
            "SELECT cursor, causal_token
             FROM patchouli_scope_frontier
             WHERE scope_json = ?1",
            [&query.scope_json],
            |row| Ok((row.get::<_, u64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(database_error)?;
    let current_cursor = current.as_ref().map_or(0, |(cursor, _)| *cursor);

    for token in &query.minimum_tokens {
        let available = transaction
            .query_row(
                "SELECT 1 FROM patchouli_causal_frontier
                 WHERE scope_json = ?1 AND causal_token = ?2",
                (&query.scope_json, token),
                |_| Ok(()),
            )
            .optional()
            .map_err(database_error)?
            .is_some();
        if !available {
            return Ok(ConsistencyAcquireOutcome::Unavailable);
        }
    }
    for session_key in &query.session_keys {
        let required = transaction
            .query_row(
                "SELECT cursor FROM patchouli_session_frontier
                 WHERE scope_json = ?1 AND session_key_json = ?2",
                (&query.scope_json, session_key),
                |row| row.get::<_, u64>(0),
            )
            .optional()
            .map_err(database_error)?
            .unwrap_or(0);
        if required > current_cursor {
            return Ok(ConsistencyAcquireOutcome::Unavailable);
        }
    }
    for session_key in &query.session_keys {
        transaction
            .execute(
                "INSERT INTO patchouli_session_frontier (
                    scope_json, session_key_json, cursor
                 ) VALUES (?1, ?2, ?3)
                 ON CONFLICT (scope_json, session_key_json)
                 DO UPDATE SET cursor = max(cursor, excluded.cursor)",
                (&query.scope_json, session_key, current_cursor),
            )
            .map_err(database_error)?;
    }
    transaction.commit().map_err(database_error)?;
    Ok(ConsistencyAcquireOutcome::Acquired {
        causal_token: current.map(|(_, token)| token),
    })
}

fn read_changes(
    connection: &rusqlite::Connection,
    query: &ChangeQuery,
) -> Result<ChangePage, ProviderError> {
    connection
        .execute(
            "INSERT INTO patchouli_change_retention (scope_json, pruned_through_cursor)
             SELECT scope_json, max(cursor)
             FROM patchouli_change
             WHERE recorded_at_unix_ms < ?1
             GROUP BY scope_json
             ON CONFLICT (scope_json) DO UPDATE SET
                pruned_through_cursor = max(
                    patchouli_change_retention.pruned_through_cursor,
                    excluded.pruned_through_cursor
                )",
            [query.retained_after_unix_ms],
        )
        .map_err(database_error)?;
    connection
        .execute(
            "DELETE FROM patchouli_change WHERE recorded_at_unix_ms < ?1",
            [query.retained_after_unix_ms],
        )
        .map_err(database_error)?;
    let pruned_through = connection
        .query_row(
            "SELECT pruned_through_cursor
             FROM patchouli_change_retention
             WHERE scope_json = ?1",
            [&query.scope_json],
            |row| row.get::<_, u64>(0),
        )
        .optional()
        .map_err(database_error)?;
    let current_cursor = connection
        .query_row(
            "SELECT cursor FROM patchouli_scope_frontier WHERE scope_json = ?1",
            [&query.scope_json],
            |row| row.get::<_, u64>(0),
        )
        .optional()
        .map_err(database_error)?
        .unwrap_or(0);
    let rows = connection
        .prepare(
            "SELECT cursor, entity_type, entity_id, kind, head_versions_json, event_meta_json
             FROM patchouli_change
             WHERE scope_json = ?1 AND cursor > ?2
             ORDER BY cursor",
        )
        .map_err(database_error)?
        .query_map((&query.scope_json, query.after_cursor), |row| {
            Ok((
                row.get::<_, u64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;
    let mut changes = Vec::new();
    for (cursor, entity_type, entity_id, kind, heads_json, event_meta_json) in rows {
        if query
            .entity_types
            .as_ref()
            .is_some_and(|types| !types.contains(&entity_type))
            || query
                .entity_ids
                .as_ref()
                .is_some_and(|ids| !ids.contains(&entity_id))
        {
            continue;
        }
        let kind = match kind.as_str() {
            "conflicted" => StoredChangeKind::Conflicted,
            "created" => StoredChangeKind::Created,
            "deleted" => StoredChangeKind::Deleted,
            "resolved" => StoredChangeKind::Resolved,
            "updated" => StoredChangeKind::Updated,
            other => return Err(ProviderError::new(format!("unknown change kind {other:?}"))),
        };
        changes.push(StoredChange {
            cursor,
            entity_type,
            entity_id,
            kind,
            head_versions: serde_json::from_str(&heads_json).map_err(|error| {
                ProviderError::new(format!("invalid stored head versions: {error}"))
            })?,
            event_meta_json,
        });
        if changes.len() == query.limit {
            break;
        }
    }
    Ok(ChangePage {
        oldest_cursor: pruned_through.map(|cursor| cursor.saturating_add(1)),
        current_cursor,
        changes,
    })
}

fn retrieve_entities(
    connection: &rusqlite::Connection,
    query: &RetrieveQuery,
) -> Result<Vec<RetrievedEntity>, ProviderError> {
    let keys = connection
        .prepare(
            "SELECT DISTINCT version.entity_type, version.entity_id,
                    max(version.recorded_at_unix_ms) AS newest
             FROM patchouli_entity_version AS version
             INNER JOIN patchouli_entity_head AS head USING (
                scope_json, entity_type, entity_id, version
             )
             WHERE version.scope_json = ?1
               AND version.state = 'active'
               AND instr(lower(version.value_json), lower(?2)) > 0
             GROUP BY version.entity_type, version.entity_id
             ORDER BY newest DESC",
        )
        .map_err(database_error)?
        .query_map((&query.scope_json, &query.query), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;
    let mut entities = Vec::new();
    for (entity_type, entity_id) in keys {
        if query
            .entity_types
            .as_ref()
            .is_some_and(|types| !types.contains(&entity_type))
        {
            continue;
        }
        let key = EntityKey {
            scope_json: query.scope_json.clone(),
            entity_type,
            entity_id,
        };
        if let Some(snapshot) = read_entity_snapshot(connection, &key)? {
            entities.push(RetrievedEntity { key, snapshot });
        }
        if entities.len() == query.limit {
            break;
        }
    }
    Ok(entities)
}

fn read_entity_snapshot(
    connection: &rusqlite::Connection,
    key: &EntityKey,
) -> Result<Option<EntitySnapshot>, ProviderError> {
    let mut head_versions = connection
        .prepare(
            "SELECT version
             FROM patchouli_entity_head
             WHERE scope_json = ?1 AND entity_type = ?2 AND entity_id = ?3
             ORDER BY version",
        )
        .map_err(database_error)?
        .query_map((&key.scope_json, &key.entity_type, &key.entity_id), |row| {
            row.get(0)
        })
        .map_err(database_error)?
        .collect::<Result<Vec<String>, _>>()
        .map_err(database_error)?;
    if head_versions.is_empty() {
        return Ok(None);
    }
    head_versions.sort();

    let stored = connection
        .prepare(
            "SELECT version, state, value_json
             FROM patchouli_entity_version
             WHERE scope_json = ?1
               AND entity_type = ?2
               AND entity_id = ?3
               AND published_cursor IS NOT NULL
             ORDER BY recorded_at_unix_ms, version",
        )
        .map_err(database_error)?
        .query_map((&key.scope_json, &key.entity_type, &key.entity_id), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;

    let mut versions = Vec::with_capacity(stored.len());
    for (version, state, value_json) in stored {
        let state = match state.as_str() {
            "active" => StoredVersionState::Active,
            "deleted" => StoredVersionState::Deleted,
            other => {
                return Err(ProviderError::new(format!(
                    "unknown stored entity state {other:?}"
                )));
            }
        };
        versions.push(StoredEntityVersion {
            crdt_fields: read_crdt_fields(connection, key, &version)?,
            version,
            state,
            value_json,
        });
    }

    Ok(Some(EntitySnapshot {
        head_versions,
        versions,
    }))
}

fn read_crdt_fields(
    connection: &rusqlite::Connection,
    key: &EntityKey,
    version: &str,
) -> Result<Vec<StoredCrdtField>, ProviderError> {
    let paths = connection
        .prepare(
            "SELECT DISTINCT field_path
             FROM patchouli_entity_crdt_head
             WHERE scope_json = ?1
               AND entity_type = ?2
               AND entity_id = ?3
               AND version = ?4
             ORDER BY field_path",
        )
        .map_err(database_error)?
        .query_map(
            (&key.scope_json, &key.entity_type, &key.entity_id, version),
            |row| row.get::<_, String>(0),
        )
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;

    let mut fields = Vec::with_capacity(paths.len());
    for path in paths {
        let heads = connection
            .prepare(
                "SELECT change_hash
                 FROM patchouli_entity_crdt_head
                 WHERE scope_json = ?1
                   AND entity_type = ?2
                   AND entity_id = ?3
                   AND version = ?4
                   AND field_path = ?5
                 ORDER BY change_hash",
            )
            .map_err(database_error)?
            .query_map(
                (
                    &key.scope_json,
                    &key.entity_type,
                    &key.entity_id,
                    version,
                    &path,
                ),
                |row| row.get::<_, String>(0),
            )
            .map_err(database_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(database_error)?;

        let stored_changes = connection
            .prepare(
                "WITH RECURSIVE reachable(change_hash) AS (
                    SELECT change_hash
                    FROM patchouli_entity_crdt_head
                    WHERE scope_json = ?1
                      AND entity_type = ?2
                      AND entity_id = ?3
                      AND version = ?4
                      AND field_path = ?5
                    UNION
                    SELECT edge.parent_hash
                    FROM patchouli_crdt_change_parent AS edge
                    INNER JOIN reachable ON edge.change_hash = reachable.change_hash
                 )
                 SELECT change.change_hash, change.change_bytes
                 FROM patchouli_crdt_change AS change
                 INNER JOIN reachable USING (change_hash)
                 ORDER BY change.change_hash",
            )
            .map_err(database_error)?
            .query_map(
                (
                    &key.scope_json,
                    &key.entity_type,
                    &key.entity_id,
                    version,
                    &path,
                ),
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .map_err(database_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(database_error)?;

        let mut changes = Vec::with_capacity(stored_changes.len());
        for (hash, bytes) in stored_changes {
            let parents = connection
                .prepare(
                    "SELECT parent_hash
                     FROM patchouli_crdt_change_parent
                     WHERE change_hash = ?1
                     ORDER BY parent_hash",
                )
                .map_err(database_error)?
                .query_map([&hash], |row| row.get::<_, String>(0))
                .map_err(database_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(database_error)?;
            changes.push(StoredCrdtChange {
                hash,
                parents,
                bytes,
            });
        }
        fields.push(StoredCrdtField {
            path,
            heads,
            changes,
        });
    }
    Ok(fields)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StoredWorkUnitState {
    Open,
    Closing,
    PolicyMismatch,
    Committed,
    Expired,
}

fn read_entity_in_work_unit(
    connection: &mut rusqlite::Connection,
    work_unit: &WorkUnit,
    key: &EntityKey,
) -> Result<WorkUnitReadOutcome, ProviderError> {
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(database_error)?;
    let state = ensure_work_unit(&transaction, work_unit)?;
    let result = match state {
        StoredWorkUnitState::PolicyMismatch => WorkUnitReadOutcome::PolicyMismatch,
        StoredWorkUnitState::Committed => WorkUnitReadOutcome::Committed,
        StoredWorkUnitState::Expired => WorkUnitReadOutcome::Expired,
        StoredWorkUnitState::Closing => WorkUnitReadOutcome::Closing,
        StoredWorkUnitState::Open => {
            capture_work_unit_entity(&transaction, &work_unit.identity_json, key)?;
            WorkUnitReadOutcome::Open(read_work_unit_snapshot(
                &transaction,
                &work_unit.identity_json,
                key,
            )?)
        }
    };
    commit_before_deadline(transaction, work_unit.deadline_unix_ms)?;
    Ok(result)
}

fn commit_entity_in_work_unit(
    connection: &mut rusqlite::Connection,
    commit: WorkUnitCommit,
) -> Result<WorkUnitCommitOutcome, ProviderError> {
    if commit.entity.head_versions.is_empty() {
        return Err(ProviderError::new(
            "a work-unit entity commit requires at least one new head",
        ));
    }
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(database_error)?;
    match ensure_work_unit(&transaction, &commit.work_unit)? {
        StoredWorkUnitState::PolicyMismatch => {
            commit_before_deadline(transaction, commit.work_unit.deadline_unix_ms)?;
            return Ok(WorkUnitCommitOutcome::PolicyMismatch);
        }
        StoredWorkUnitState::Committed => {
            commit_before_deadline(transaction, commit.work_unit.deadline_unix_ms)?;
            return Ok(WorkUnitCommitOutcome::Committed);
        }
        StoredWorkUnitState::Expired => {
            commit_before_deadline(transaction, commit.work_unit.deadline_unix_ms)?;
            return Ok(WorkUnitCommitOutcome::Expired);
        }
        StoredWorkUnitState::Closing => {
            if commit.close {
                let unit = &commit.work_unit.identity_json;
                let conflicts = publication_conflicts(&transaction, unit)?;
                if conflicts.is_empty() {
                    let recorded_at = sqlite_time(
                        commit.entity.recorded_at_unix_ms,
                        "recorded timestamp exceeds SQLite integer range",
                    )?;
                    finish_work_unit_publication(&transaction, unit, recorded_at)?;
                    commit_before_deadline(transaction, commit.work_unit.deadline_unix_ms)?;
                    return Ok(WorkUnitCommitOutcome::Published);
                }
                commit_before_deadline(transaction, commit.work_unit.deadline_unix_ms)?;
                return Ok(WorkUnitCommitOutcome::PublicationConflict { conflicts });
            }
            commit_before_deadline(transaction, commit.work_unit.deadline_unix_ms)?;
            return Ok(WorkUnitCommitOutcome::Closing);
        }
        StoredWorkUnitState::Open => {}
    }

    let unit = &commit.work_unit.identity_json;
    let key = &commit.entity.key;
    capture_work_unit_entity(&transaction, unit, key)?;
    transaction
        .execute(
            "UPDATE patchouli_work_unit_entity
             SET conflict_policy_json = ?5,
                 causal_token = ?6,
                 event_meta_json = ?7,
                 session_keys_json = ?8,
                 close_marker = max(close_marker, ?9)
             WHERE work_unit_json = ?1
               AND scope_json = ?2
               AND entity_type = ?3
               AND entity_id = ?4",
            (
                unit,
                &key.scope_json,
                &key.entity_type,
                &key.entity_id,
                &commit.conflict_policy_json,
                &commit.entity.causal_token,
                &commit.entity.event_meta_json,
                serde_json::to_string(&commit.entity.session_keys).map_err(|error| {
                    ProviderError::new(format!("failed to encode session keys: {error}"))
                })?,
                commit.close,
            ),
        )
        .map_err(database_error)?;
    let current_heads = work_unit_heads(&transaction, unit, key)?;
    let mut expected_heads = commit.entity.expected_heads.clone();
    expected_heads.sort();
    if current_heads != expected_heads {
        return Ok(WorkUnitCommitOutcome::Conflict { current_heads });
    }

    let recorded_at = sqlite_time(
        commit.entity.recorded_at_unix_ms,
        "recorded timestamp exceeds SQLite integer range",
    )?;
    insert_entity_versions(
        &transaction,
        key,
        &commit.entity.new_versions,
        Some(unit),
        recorded_at,
    )?;
    replace_work_unit_heads(&transaction, unit, key, &commit.entity.head_versions)?;

    if let Some(idempotency) = &commit.idempotency {
        match store_work_unit_idempotency(
            &transaction,
            unit,
            idempotency,
            commit.work_unit.now_unix_ms,
        )? {
            IdempotencyReadOutcome::Missing => {}
            IdempotencyReadOutcome::Replayed { .. } if commit.close => {}
            IdempotencyReadOutcome::Replayed { result_json } => {
                return Ok(WorkUnitCommitOutcome::Replayed { result_json });
            }
            IdempotencyReadOutcome::Conflict => {
                return Ok(WorkUnitCommitOutcome::IdempotencyConflict);
            }
        }
    }

    if !commit.close {
        commit_before_deadline(transaction, commit.work_unit.deadline_unix_ms)?;
        return Ok(WorkUnitCommitOutcome::Staged);
    }

    transaction
        .execute(
            "UPDATE patchouli_work_unit SET state = 'closing' WHERE identity_json = ?1 AND state = 'open'",
            [unit],
        )
        .map_err(database_error)?;

    let conflicts = publication_conflicts(&transaction, unit)?;
    if !conflicts.is_empty() {
        commit_before_deadline(transaction, commit.work_unit.deadline_unix_ms)?;
        return Ok(WorkUnitCommitOutcome::PublicationConflict { conflicts });
    }

    finish_work_unit_publication(&transaction, unit, recorded_at)?;
    commit_before_deadline(transaction, commit.work_unit.deadline_unix_ms)?;
    Ok(WorkUnitCommitOutcome::Published)
}

fn publish_work_unit(
    connection: &mut rusqlite::Connection,
    publish: WorkUnitPublish,
) -> Result<WorkUnitCommitOutcome, ProviderError> {
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(database_error)?;
    match ensure_work_unit(&transaction, &publish.work_unit)? {
        StoredWorkUnitState::PolicyMismatch => {
            transaction.commit().map_err(database_error)?;
            return Ok(WorkUnitCommitOutcome::PolicyMismatch);
        }
        StoredWorkUnitState::Committed => {
            transaction.commit().map_err(database_error)?;
            return Ok(WorkUnitCommitOutcome::Committed);
        }
        StoredWorkUnitState::Expired => {
            transaction.commit().map_err(database_error)?;
            return Ok(WorkUnitCommitOutcome::Expired);
        }
        StoredWorkUnitState::Open => {
            return Err(ProviderError::new(
                "an open work unit cannot be published before a close marker",
            ));
        }
        StoredWorkUnitState::Closing => {}
    }

    let unit = &publish.work_unit.identity_json;
    let conflicts = publication_conflicts(&transaction, unit)?;
    if conflicts.len() != publish.resolutions.len() {
        return Ok(WorkUnitCommitOutcome::PublicationConflict { conflicts });
    }
    for conflict in &conflicts {
        let Some(resolution) = publish
            .resolutions
            .iter()
            .find(|resolution| resolution.entity.key == conflict.key)
        else {
            return Ok(WorkUnitCommitOutcome::PublicationConflict {
                conflicts: conflicts.clone(),
            });
        };
        let mut expected_published = resolution.expected_published_heads.clone();
        expected_published.sort();
        let current_heads = conflict
            .current
            .as_ref()
            .map_or_else(Vec::new, |snapshot| snapshot.head_versions.clone());
        if expected_published != current_heads {
            return Ok(WorkUnitCommitOutcome::PublicationConflict {
                conflicts: conflicts.clone(),
            });
        }
        let mut expected_staged = resolution.entity.expected_heads.clone();
        expected_staged.sort();
        if expected_staged != conflict.staged.head_versions {
            return Ok(WorkUnitCommitOutcome::Conflict {
                current_heads: conflict.staged.head_versions.clone(),
            });
        }
        if resolution.entity.head_versions.is_empty() {
            return Err(ProviderError::new(
                "a work-unit publication resolution requires at least one head",
            ));
        }
        let recorded_at = sqlite_time(
            resolution.entity.recorded_at_unix_ms,
            "resolution timestamp exceeds SQLite integer range",
        )?;
        insert_entity_versions(
            &transaction,
            &conflict.key,
            &resolution.entity.new_versions,
            Some(unit),
            recorded_at,
        )?;
        replace_work_unit_heads(
            &transaction,
            unit,
            &conflict.key,
            &resolution.entity.head_versions,
        )?;
    }

    let recorded_at = sqlite_time(
        publish.recorded_at_unix_ms,
        "publication timestamp exceeds SQLite integer range",
    )?;
    finish_work_unit_publication(&transaction, unit, recorded_at)?;
    transaction.commit().map_err(database_error)?;
    Ok(WorkUnitCommitOutcome::Published)
}

fn finish_work_unit_publication(
    transaction: &rusqlite::Transaction<'_>,
    unit: &str,
    recorded_at: i64,
) -> Result<(), ProviderError> {
    for entity in work_unit_mutated_entities(transaction, unit)? {
        let previous_heads = published_heads(transaction, &entity)?;
        let heads = direct_work_unit_heads(transaction, unit, &entity)?;
        replace_published_heads(transaction, &entity, &heads)?;
        let kind = derive_change_kind(transaction, &entity, &previous_heads, &heads)?;
        let (causal_token, event_meta_json, session_keys_json) = transaction
            .query_row(
                "SELECT causal_token, event_meta_json, session_keys_json
                 FROM patchouli_work_unit_entity
                 WHERE work_unit_json = ?1
                   AND scope_json = ?2
                   AND entity_type = ?3
                   AND entity_id = ?4",
                (
                    unit,
                    &entity.scope_json,
                    &entity.entity_type,
                    &entity.entity_id,
                ),
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .map_err(database_error)?;
        let cursor = insert_change(
            transaction,
            &entity,
            kind,
            &heads,
            &causal_token,
            &event_meta_json,
            recorded_at,
        )?;
        let session_keys: Vec<String> = serde_json::from_str(&session_keys_json)
            .map_err(|error| ProviderError::new(format!("invalid stored session keys: {error}")))?;
        advance_sessions(transaction, &entity.scope_json, &session_keys, cursor)?;
        publish_work_unit_versions(transaction, unit, &entity, cursor)?;
    }
    transaction
        .execute(
            "INSERT INTO patchouli_idempotency (
                identity_json, request_json, result_json, expires_at_unix_ms
             )
             SELECT identity_json, request_json, result_json, expires_at_unix_ms
             FROM patchouli_work_unit_idempotency
             WHERE work_unit_json = ?1 AND expires_at_unix_ms > ?2",
            (unit, recorded_at),
        )
        .map_err(database_error)?;
    transaction
        .execute(
            "DELETE FROM patchouli_work_unit_idempotency WHERE work_unit_json = ?1",
            [unit],
        )
        .map_err(database_error)?;
    transaction
        .execute(
            "UPDATE patchouli_work_unit
             SET state = 'committed', closed_at_unix_ms = ?2
             WHERE identity_json = ?1",
            (unit, recorded_at),
        )
        .map_err(database_error)?;
    Ok(())
}

fn ensure_work_unit(
    transaction: &rusqlite::Transaction<'_>,
    work_unit: &WorkUnit,
) -> Result<StoredWorkUnitState, ProviderError> {
    let existing = transaction
        .query_row(
            "SELECT state, expires_at_unix_ms, policy_json, expiry_action, scope_json
             FROM patchouli_work_unit
             WHERE identity_json = ?1",
            [&work_unit.identity_json],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(database_error)?;
    let expected_expiry_action = match work_unit.expiry_action {
        WorkUnitExpiryAction::Discard => "discard",
    };
    let Some((state, expires_at, policy_json, expiry_action, scope_json)) = existing else {
        let now = sqlite_time(
            work_unit.now_unix_ms,
            "work-unit timestamp exceeds SQLite integer range",
        )?;
        let expires = sqlite_time(
            work_unit.expires_at_unix_ms,
            "work-unit expiry exceeds SQLite integer range",
        )?;
        if expires <= now {
            return Err(ProviderError::new(
                "work-unit expiry must be later than its opening time",
            ));
        }
        transaction
            .execute(
                "INSERT INTO patchouli_work_unit (
                    identity_json,
                    scope_json,
                    state,
                    policy_json,
                    expiry_action,
                    opened_at_unix_ms,
                    expires_at_unix_ms,
                    baseline_cursor,
                    closed_at_unix_ms
                 ) VALUES (
                    ?1,
                    ?2,
                    'open',
                    ?3,
                    ?4,
                    ?5,
                    ?6,
                    COALESCE((
                        SELECT cursor FROM patchouli_scope_frontier WHERE scope_json = ?2
                    ), 0),
                    NULL
                 )",
                (
                    &work_unit.identity_json,
                    &work_unit.scope_json,
                    &work_unit.policy_json,
                    expected_expiry_action,
                    now,
                    expires,
                ),
            )
            .map_err(database_error)?;
        return Ok(StoredWorkUnitState::Open);
    };

    if scope_json != work_unit.scope_json
        || policy_json != work_unit.policy_json
        || expiry_action != expected_expiry_action
    {
        return Ok(StoredWorkUnitState::PolicyMismatch);
    }
    match state.as_str() {
        "committed" => Ok(StoredWorkUnitState::Committed),
        "expired" => Ok(StoredWorkUnitState::Expired),
        "closing" if work_unit.now_unix_ms < expires_at => Ok(StoredWorkUnitState::Closing),
        "closing" => {
            expire_work_unit(transaction, &work_unit.identity_json, work_unit.now_unix_ms)?;
            Ok(StoredWorkUnitState::Expired)
        }
        "open" if work_unit.now_unix_ms < expires_at => Ok(StoredWorkUnitState::Open),
        "open" => {
            expire_work_unit(transaction, &work_unit.identity_json, work_unit.now_unix_ms)?;
            Ok(StoredWorkUnitState::Expired)
        }
        other => Err(ProviderError::new(format!(
            "unknown stored work-unit state {other:?}"
        ))),
    }
}

fn sweep_expired_work_units(connection: &mut rusqlite::Connection) -> Result<(), ProviderError> {
    let now = unix_time_ms()?;
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(database_error)?;
    expire_open_work_units(&transaction, now)?;
    transaction.commit().map_err(database_error)
}

fn expire_open_work_units(
    transaction: &rusqlite::Transaction<'_>,
    now_unix_ms: u64,
) -> Result<(), ProviderError> {
    let expired = transaction
        .prepare(
            "SELECT identity_json
             FROM patchouli_work_unit
             WHERE state IN ('open', 'closing')
               AND expiry_action = 'discard'
               AND expires_at_unix_ms <= ?1
             ORDER BY identity_json",
        )
        .map_err(database_error)?
        .query_map([now_unix_ms], |row| row.get::<_, String>(0))
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;
    for identity in &expired {
        expire_work_unit(transaction, identity, now_unix_ms)?;
    }
    Ok(())
}

fn expire_work_unit(
    transaction: &rusqlite::Transaction<'_>,
    identity: &str,
    now_unix_ms: u64,
) -> Result<(), ProviderError> {
    let now = sqlite_time(
        now_unix_ms,
        "work-unit timestamp exceeds SQLite integer range",
    )?;
    transaction
        .execute(
            "UPDATE patchouli_work_unit
             SET state = 'expired', closed_at_unix_ms = ?2
             WHERE identity_json = ?1 AND state IN ('open', 'closing')",
            (identity, now),
        )
        .map_err(database_error)?;
    transaction
        .execute(
            "DELETE FROM patchouli_entity_version WHERE work_unit_json = ?1",
            [identity],
        )
        .map_err(database_error)?;
    transaction
        .execute(
            "DELETE FROM patchouli_work_unit_idempotency WHERE work_unit_json = ?1",
            [identity],
        )
        .map_err(database_error)?;
    transaction
        .execute(
            "WITH RECURSIVE reachable(change_hash) AS (
                SELECT change_hash FROM patchouli_entity_crdt_head
                UNION
                SELECT edge.parent_hash
                FROM patchouli_crdt_change_parent AS edge
                INNER JOIN reachable ON edge.change_hash = reachable.change_hash
             )
             DELETE FROM patchouli_crdt_change
             WHERE change_hash NOT IN (SELECT change_hash FROM reachable)",
            [],
        )
        .map_err(database_error)?;
    Ok(())
}

fn capture_work_unit_entity(
    transaction: &rusqlite::Transaction<'_>,
    unit: &str,
    key: &EntityKey,
) -> Result<(), ProviderError> {
    let inserted = transaction
        .execute(
            "INSERT OR IGNORE INTO patchouli_work_unit_entity (
                work_unit_json, scope_json, entity_type, entity_id
             ) VALUES (?1, ?2, ?3, ?4)",
            (unit, &key.scope_json, &key.entity_type, &key.entity_id),
        )
        .map_err(database_error)?;
    if inserted == 0 {
        return Ok(());
    }
    transaction
        .execute(
            "WITH cutoff(cursor) AS (
                SELECT baseline_cursor
                FROM patchouli_work_unit
                WHERE identity_json = ?1
             ),
             baseline(head_versions_json) AS (
                SELECT history.head_versions_json
                FROM patchouli_entity_head_history AS history, cutoff
                WHERE history.scope_json = ?2
                  AND history.entity_type = ?3
                  AND history.entity_id = ?4
                  AND history.cursor <= cutoff.cursor
                ORDER BY history.cursor DESC
                LIMIT 1
             )
             INSERT INTO patchouli_work_unit_base_version (
                work_unit_json,
                scope_json,
                entity_type,
                entity_id,
                version,
                is_head
             )
             SELECT
                ?1,
                version.scope_json,
                version.entity_type,
                version.entity_id,
                version.version,
                CASE WHEN EXISTS (
                    SELECT 1
                    FROM baseline, json_each(baseline.head_versions_json) AS head
                    WHERE head.value = version.version
                ) THEN 1 ELSE 0 END
             FROM patchouli_entity_version AS version, cutoff
             WHERE version.scope_json = ?2
               AND version.entity_type = ?3
               AND version.entity_id = ?4
               AND version.published_cursor IS NOT NULL
               AND version.published_cursor <= cutoff.cursor",
            (unit, &key.scope_json, &key.entity_type, &key.entity_id),
        )
        .map_err(database_error)?;
    Ok(())
}

fn read_work_unit_snapshot(
    transaction: &rusqlite::Transaction<'_>,
    unit: &str,
    key: &EntityKey,
) -> Result<Option<EntitySnapshot>, ProviderError> {
    let head_versions = work_unit_heads(transaction, unit, key)?;
    if head_versions.is_empty() {
        return Ok(None);
    }
    let stored = transaction
        .prepare(
            "SELECT
                version.version,
                version.state,
                version.value_json,
                version.recorded_at_unix_ms
             FROM patchouli_entity_version AS version
             INNER JOIN patchouli_work_unit_base_version AS base USING (
                scope_json, entity_type, entity_id, version
             )
             WHERE base.work_unit_json = ?1
               AND version.scope_json = ?2
               AND version.entity_type = ?3
               AND version.entity_id = ?4
             UNION
             SELECT version, state, value_json, recorded_at_unix_ms
             FROM patchouli_entity_version
             WHERE work_unit_json = ?1
               AND scope_json = ?2
               AND entity_type = ?3
               AND entity_id = ?4
             ORDER BY recorded_at_unix_ms, version",
        )
        .map_err(database_error)?
        .query_map(
            (unit, &key.scope_json, &key.entity_type, &key.entity_id),
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;
    let mut versions = Vec::with_capacity(stored.len());
    for (version, state, value_json) in stored {
        let state = match state.as_str() {
            "active" => StoredVersionState::Active,
            "deleted" => StoredVersionState::Deleted,
            other => {
                return Err(ProviderError::new(format!(
                    "unknown stored entity state {other:?}"
                )));
            }
        };
        versions.push(StoredEntityVersion {
            crdt_fields: read_crdt_fields(transaction, key, &version)?,
            version,
            state,
            value_json,
        });
    }
    Ok(Some(EntitySnapshot {
        head_versions,
        versions,
    }))
}

fn work_unit_heads(
    transaction: &rusqlite::Transaction<'_>,
    unit: &str,
    key: &EntityKey,
) -> Result<Vec<String>, ProviderError> {
    let staged = direct_work_unit_heads(transaction, unit, key)?;
    if staged.is_empty() {
        work_unit_base_heads(transaction, unit, key)
    } else {
        Ok(staged)
    }
}

fn direct_work_unit_heads(
    transaction: &rusqlite::Transaction<'_>,
    unit: &str,
    key: &EntityKey,
) -> Result<Vec<String>, ProviderError> {
    query_versions(
        transaction,
        "SELECT version
         FROM patchouli_work_unit_head
         WHERE work_unit_json = ?1
           AND scope_json = ?2
           AND entity_type = ?3
           AND entity_id = ?4
         ORDER BY version",
        unit,
        key,
    )
}

fn work_unit_base_heads(
    transaction: &rusqlite::Transaction<'_>,
    unit: &str,
    key: &EntityKey,
) -> Result<Vec<String>, ProviderError> {
    query_versions(
        transaction,
        "SELECT version
         FROM patchouli_work_unit_base_version
         WHERE work_unit_json = ?1
           AND scope_json = ?2
           AND entity_type = ?3
           AND entity_id = ?4
           AND is_head = 1
         ORDER BY version",
        unit,
        key,
    )
}

fn query_versions(
    transaction: &rusqlite::Transaction<'_>,
    sql: &str,
    unit: &str,
    key: &EntityKey,
) -> Result<Vec<String>, ProviderError> {
    transaction
        .prepare(sql)
        .map_err(database_error)?
        .query_map(
            (unit, &key.scope_json, &key.entity_type, &key.entity_id),
            |row| row.get(0),
        )
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)
}

fn published_heads(
    transaction: &rusqlite::Transaction<'_>,
    key: &EntityKey,
) -> Result<Vec<String>, ProviderError> {
    transaction
        .prepare(
            "SELECT version
             FROM patchouli_entity_head
             WHERE scope_json = ?1 AND entity_type = ?2 AND entity_id = ?3
             ORDER BY version",
        )
        .map_err(database_error)?
        .query_map((&key.scope_json, &key.entity_type, &key.entity_id), |row| {
            row.get(0)
        })
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)
}

fn replace_work_unit_heads(
    transaction: &rusqlite::Transaction<'_>,
    unit: &str,
    key: &EntityKey,
    heads: &[String],
) -> Result<(), ProviderError> {
    let heads = unique_heads(heads)?;
    transaction
        .execute(
            "DELETE FROM patchouli_work_unit_head
             WHERE work_unit_json = ?1
               AND scope_json = ?2
               AND entity_type = ?3
               AND entity_id = ?4",
            (unit, &key.scope_json, &key.entity_type, &key.entity_id),
        )
        .map_err(database_error)?;
    for version in heads {
        transaction
            .execute(
                "INSERT INTO patchouli_work_unit_head (
                    work_unit_json, scope_json, entity_type, entity_id, version
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                (
                    unit,
                    &key.scope_json,
                    &key.entity_type,
                    &key.entity_id,
                    version,
                ),
            )
            .map_err(database_error)?;
    }
    Ok(())
}

fn replace_published_heads(
    transaction: &rusqlite::Transaction<'_>,
    key: &EntityKey,
    heads: &[String],
) -> Result<(), ProviderError> {
    transaction
        .execute(
            "DELETE FROM patchouli_entity_head
             WHERE scope_json = ?1 AND entity_type = ?2 AND entity_id = ?3",
            (&key.scope_json, &key.entity_type, &key.entity_id),
        )
        .map_err(database_error)?;
    for version in unique_heads(heads)? {
        transaction
            .execute(
                "INSERT INTO patchouli_entity_head (
                    scope_json, entity_type, entity_id, version
                 ) VALUES (?1, ?2, ?3, ?4)",
                (&key.scope_json, &key.entity_type, &key.entity_id, version),
            )
            .map_err(database_error)?;
    }
    Ok(())
}

fn work_unit_mutated_entities(
    transaction: &rusqlite::Transaction<'_>,
    unit: &str,
) -> Result<Vec<EntityKey>, ProviderError> {
    transaction
        .prepare(
            "SELECT scope_json, entity_type, entity_id
             FROM patchouli_work_unit_entity AS entity
             WHERE work_unit_json = ?1
               AND EXISTS (
                    SELECT 1
                    FROM patchouli_work_unit_head AS head
                    WHERE head.work_unit_json = entity.work_unit_json
                      AND head.scope_json = entity.scope_json
                      AND head.entity_type = entity.entity_type
                      AND head.entity_id = entity.entity_id
               )
             ORDER BY close_marker, scope_json, entity_type, entity_id",
        )
        .map_err(database_error)?
        .query_map([unit], |row| {
            Ok(EntityKey {
                scope_json: row.get(0)?,
                entity_type: row.get(1)?,
                entity_id: row.get(2)?,
            })
        })
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)
}

fn publication_conflicts(
    transaction: &rusqlite::Transaction<'_>,
    unit: &str,
) -> Result<Vec<WorkUnitConflict>, ProviderError> {
    let mut conflicts = Vec::new();
    for key in work_unit_mutated_entities(transaction, unit)? {
        let baseline_heads = work_unit_base_heads(transaction, unit, &key)?;
        let current = read_entity_snapshot(transaction, &key)?;
        let current_heads = current
            .as_ref()
            .map_or_else(Vec::new, |snapshot| snapshot.head_versions.clone());
        if current_heads == baseline_heads {
            continue;
        }
        let staged = read_work_unit_snapshot(transaction, unit, &key)?
            .ok_or_else(|| ProviderError::new("mutated work-unit entity has no staged snapshot"))?;
        let conflict_policy_json = transaction
            .query_row(
                "SELECT conflict_policy_json
                 FROM patchouli_work_unit_entity
                 WHERE work_unit_json = ?1
                   AND scope_json = ?2
                   AND entity_type = ?3
                   AND entity_id = ?4",
                (unit, &key.scope_json, &key.entity_type, &key.entity_id),
                |row| row.get::<_, Option<String>>(0),
            )
            .map_err(database_error)?
            .ok_or_else(|| ProviderError::new("mutated work-unit entity has no conflict policy"))?;
        conflicts.push(WorkUnitConflict {
            key,
            baseline_heads,
            staged,
            current,
            conflict_policy_json,
        });
    }
    Ok(conflicts)
}

fn derive_change_kind(
    transaction: &rusqlite::Transaction<'_>,
    key: &EntityKey,
    baseline: &[String],
    heads: &[String],
) -> Result<StoredChangeKind, ProviderError> {
    if baseline.is_empty() {
        return Ok(StoredChangeKind::Created);
    }
    if heads.len() > 1 {
        return Ok(StoredChangeKind::Conflicted);
    }
    if baseline.len() > 1 {
        return Ok(StoredChangeKind::Resolved);
    }
    let state: String = transaction
        .query_row(
            "SELECT state
             FROM patchouli_entity_version
             WHERE scope_json = ?1
               AND entity_type = ?2
               AND entity_id = ?3
               AND version = ?4",
            (
                &key.scope_json,
                &key.entity_type,
                &key.entity_id,
                heads.first().ok_or_else(|| {
                    ProviderError::new("a published entity requires at least one head")
                })?,
            ),
            |row| row.get(0),
        )
        .map_err(database_error)?;
    match state.as_str() {
        "deleted" => Ok(StoredChangeKind::Deleted),
        "active" => Ok(StoredChangeKind::Updated),
        other => Err(ProviderError::new(format!(
            "unknown stored entity state {other:?}"
        ))),
    }
}

fn insert_change(
    transaction: &rusqlite::Transaction<'_>,
    key: &EntityKey,
    kind: StoredChangeKind,
    heads: &[String],
    causal_token: &str,
    event_meta_json: &str,
    recorded_at: i64,
) -> Result<i64, ProviderError> {
    let head_versions_json = serde_json::to_string(heads)
        .map_err(|error| ProviderError::new(format!("failed to encode head versions: {error}")))?;
    transaction
        .execute(
            "INSERT INTO patchouli_change (
                scope_json,
                entity_type,
                entity_id,
                kind,
                head_versions_json,
                causal_token,
                event_meta_json,
                recorded_at_unix_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            (
                &key.scope_json,
                &key.entity_type,
                &key.entity_id,
                stored_change_kind(kind),
                &head_versions_json,
                causal_token,
                event_meta_json,
                recorded_at,
            ),
        )
        .map_err(database_error)?;
    let cursor = transaction.last_insert_rowid();
    transaction
        .execute(
            "INSERT INTO patchouli_entity_head_history (
                scope_json, entity_type, entity_id, cursor, head_versions_json
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                &key.scope_json,
                &key.entity_type,
                &key.entity_id,
                cursor,
                &head_versions_json,
            ),
        )
        .map_err(database_error)?;
    transaction
        .execute(
            "INSERT INTO patchouli_causal_frontier (causal_token, scope_json, cursor)
             VALUES (?1, ?2, ?3)",
            (causal_token, &key.scope_json, cursor),
        )
        .map_err(database_error)?;
    transaction
        .execute(
            "INSERT INTO patchouli_scope_frontier (scope_json, cursor, causal_token)
             VALUES (?1, ?2, ?3)
             ON CONFLICT (scope_json) DO UPDATE SET
                cursor = excluded.cursor,
                causal_token = excluded.causal_token",
            (&key.scope_json, cursor, causal_token),
        )
        .map_err(database_error)?;
    Ok(cursor)
}

fn publish_work_unit_versions(
    transaction: &rusqlite::Transaction<'_>,
    unit: &str,
    key: &EntityKey,
    cursor: i64,
) -> Result<(), ProviderError> {
    transaction
        .execute(
            "UPDATE patchouli_entity_version
             SET work_unit_json = NULL, published_cursor = ?5
             WHERE work_unit_json = ?1
               AND scope_json = ?2
               AND entity_type = ?3
               AND entity_id = ?4",
            (
                unit,
                &key.scope_json,
                &key.entity_type,
                &key.entity_id,
                cursor,
            ),
        )
        .map_err(database_error)?;
    Ok(())
}

fn publish_entity_versions(
    transaction: &rusqlite::Transaction<'_>,
    key: &EntityKey,
    versions: &[StoredEntityVersion],
    cursor: i64,
) -> Result<(), ProviderError> {
    for version in versions {
        transaction
            .execute(
                "UPDATE patchouli_entity_version
                 SET published_cursor = ?5
                 WHERE scope_json = ?1
                   AND entity_type = ?2
                   AND entity_id = ?3
                   AND version = ?4",
                (
                    &key.scope_json,
                    &key.entity_type,
                    &key.entity_id,
                    &version.version,
                    cursor,
                ),
            )
            .map_err(database_error)?;
    }
    Ok(())
}

fn unique_heads(heads: &[String]) -> Result<Vec<&String>, ProviderError> {
    let mut unique = heads.iter().collect::<Vec<_>>();
    unique.sort();
    unique.dedup();
    if unique.len() != heads.len() {
        return Err(ProviderError::new("entity head versions must be unique"));
    }
    if unique.is_empty() {
        return Err(ProviderError::new(
            "an entity commit requires at least one head",
        ));
    }
    Ok(unique)
}

fn sqlite_time(value: u64, message: &str) -> Result<i64, ProviderError> {
    i64::try_from(value).map_err(|_| ProviderError::new(message))
}

fn insert_entity_versions(
    transaction: &rusqlite::Transaction<'_>,
    key: &EntityKey,
    versions: &[StoredEntityVersion],
    work_unit: Option<&str>,
    recorded_at: i64,
) -> Result<(), ProviderError> {
    for version in versions {
        let state = match version.state {
            StoredVersionState::Active => "active",
            StoredVersionState::Deleted => "deleted",
        };
        transaction
            .execute(
                "INSERT INTO patchouli_entity_version (
                    scope_json,
                    entity_type,
                    entity_id,
                    version,
                    state,
                    value_json,
                    work_unit_json,
                    published_cursor,
                    recorded_at_unix_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8)",
                rusqlite::params![
                    key.scope_json,
                    key.entity_type,
                    key.entity_id,
                    version.version,
                    state,
                    version.value_json,
                    work_unit,
                    recorded_at,
                ],
            )
            .map_err(database_error)?;

        for field in &version.crdt_fields {
            for change in &field.changes {
                transaction
                    .execute(
                        "INSERT OR IGNORE INTO patchouli_crdt_change (
                            change_hash,
                            change_bytes
                         ) VALUES (?1, ?2)",
                        (&change.hash, &change.bytes),
                    )
                    .map_err(database_error)?;
            }
            for change in &field.changes {
                for parent in &change.parents {
                    transaction
                        .execute(
                            "INSERT OR IGNORE INTO patchouli_crdt_change_parent (
                                change_hash,
                                parent_hash
                             ) VALUES (?1, ?2)",
                            (&change.hash, parent),
                        )
                        .map_err(database_error)?;
                }
            }
            for head in &field.heads {
                transaction
                    .execute(
                        "INSERT INTO patchouli_entity_crdt_head (
                            scope_json,
                            entity_type,
                            entity_id,
                            version,
                            field_path,
                            change_hash
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        (
                            &key.scope_json,
                            &key.entity_type,
                            &key.entity_id,
                            &version.version,
                            &field.path,
                            head,
                        ),
                    )
                    .map_err(database_error)?;
            }
        }
    }
    Ok(())
}

fn commit_entity_transaction(
    connection: &mut rusqlite::Connection,
    commit: EntityCommit,
) -> Result<EntityCommitOutcome, ProviderError> {
    if commit.head_versions.is_empty() {
        return Err(ProviderError::new(
            "an entity commit requires at least one new head",
        ));
    }
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(database_error)?;
    let deadline = commit.deadline_unix_ms;
    let outcome = commit_entity_in_transaction(&transaction, commit)?;
    if outcome == EntityCommitOutcome::Committed {
        commit_before_deadline(transaction, deadline)?;
    }
    Ok(outcome)
}

fn commit_entity_idempotent(
    connection: &mut rusqlite::Connection,
    commit: EntityCommit,
    idempotency: IdempotencyRecord,
    now_unix_ms: u64,
) -> Result<IdempotentCommitOutcome, ProviderError> {
    let deadline = commit.deadline_unix_ms;
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(database_error)?;
    transaction
        .execute(
            "DELETE FROM patchouli_idempotency WHERE expires_at_unix_ms <= ?1",
            [now_unix_ms],
        )
        .map_err(database_error)?;
    transaction
        .execute(
            "DELETE FROM patchouli_work_unit_idempotency WHERE expires_at_unix_ms <= ?1",
            [now_unix_ms],
        )
        .map_err(database_error)?;
    if let Some((request_json, result_json)) = transaction
        .query_row(
            "SELECT request_json, result_json
             FROM patchouli_idempotency
             WHERE identity_json = ?1",
            [&idempotency.identity_json],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(database_error)?
    {
        return Ok(if request_json == idempotency.request_json {
            IdempotentCommitOutcome::Replayed { result_json }
        } else {
            IdempotentCommitOutcome::IdempotencyConflict
        });
    }
    let staged = transaction
        .query_row(
            "SELECT 1 FROM patchouli_work_unit_idempotency WHERE identity_json = ?1",
            [&idempotency.identity_json],
            |_| Ok(()),
        )
        .optional()
        .map_err(database_error)?;
    if staged.is_some() {
        return Ok(IdempotentCommitOutcome::IdempotencyConflict);
    }
    match commit_entity_in_transaction(&transaction, commit)? {
        EntityCommitOutcome::Conflict { current_heads } => {
            return Ok(IdempotentCommitOutcome::EntityConflict { current_heads });
        }
        EntityCommitOutcome::Committed => {}
    }
    transaction
        .execute(
            "INSERT INTO patchouli_idempotency (
                identity_json, request_json, result_json, expires_at_unix_ms
             ) VALUES (?1, ?2, ?3, ?4)",
            (
                &idempotency.identity_json,
                &idempotency.request_json,
                &idempotency.result_json,
                idempotency.expires_at_unix_ms,
            ),
        )
        .map_err(database_error)?;
    commit_before_deadline(transaction, deadline)?;
    Ok(IdempotentCommitOutcome::Committed)
}

fn store_work_unit_idempotency(
    transaction: &rusqlite::Transaction<'_>,
    work_unit: &str,
    idempotency: &IdempotencyRecord,
    now_unix_ms: u64,
) -> Result<IdempotencyReadOutcome, ProviderError> {
    transaction
        .execute(
            "DELETE FROM patchouli_idempotency WHERE expires_at_unix_ms <= ?1",
            [now_unix_ms],
        )
        .map_err(database_error)?;
    transaction
        .execute(
            "DELETE FROM patchouli_work_unit_idempotency WHERE expires_at_unix_ms <= ?1",
            [now_unix_ms],
        )
        .map_err(database_error)?;
    if let Some((request_json, result_json)) = transaction
        .query_row(
            "SELECT request_json, result_json
             FROM patchouli_idempotency
             WHERE identity_json = ?1",
            [&idempotency.identity_json],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(database_error)?
    {
        return Ok(if request_json == idempotency.request_json {
            IdempotencyReadOutcome::Replayed { result_json }
        } else {
            IdempotencyReadOutcome::Conflict
        });
    }
    if let Some((stored_unit, request_json, result_json)) = transaction
        .query_row(
            "SELECT work_unit_json, request_json, result_json
             FROM patchouli_work_unit_idempotency
             WHERE identity_json = ?1",
            [&idempotency.identity_json],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(database_error)?
    {
        return Ok(
            if stored_unit == work_unit && request_json == idempotency.request_json {
                IdempotencyReadOutcome::Replayed { result_json }
            } else {
                IdempotencyReadOutcome::Conflict
            },
        );
    }
    transaction
        .execute(
            "INSERT INTO patchouli_work_unit_idempotency (
                identity_json, work_unit_json, request_json, result_json, expires_at_unix_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                &idempotency.identity_json,
                work_unit,
                &idempotency.request_json,
                &idempotency.result_json,
                idempotency.expires_at_unix_ms,
            ),
        )
        .map_err(database_error)?;
    Ok(IdempotencyReadOutcome::Missing)
}

fn commit_entity_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    commit: EntityCommit,
) -> Result<EntityCommitOutcome, ProviderError> {
    let mut current_heads = transaction
        .prepare(
            "SELECT version
             FROM patchouli_entity_head
             WHERE scope_json = ?1 AND entity_type = ?2 AND entity_id = ?3
             ORDER BY version",
        )
        .map_err(database_error)?
        .query_map(
            (
                &commit.key.scope_json,
                &commit.key.entity_type,
                &commit.key.entity_id,
            ),
            |row| row.get::<_, String>(0),
        )
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;
    current_heads.sort();
    let mut expected_heads = commit.expected_heads.clone();
    expected_heads.sort();
    if current_heads != expected_heads {
        return Ok(EntityCommitOutcome::Conflict { current_heads });
    }

    let recorded_at = i64::try_from(commit.recorded_at_unix_ms)
        .map_err(|_| ProviderError::new("recorded timestamp exceeds SQLite integer range"))?;
    insert_entity_versions(
        transaction,
        &commit.key,
        &commit.new_versions,
        None,
        recorded_at,
    )?;

    replace_published_heads(transaction, &commit.key, &commit.head_versions)?;
    let cursor = insert_change(
        transaction,
        &commit.key,
        commit.change_kind,
        &commit.head_versions,
        &commit.causal_token,
        &commit.event_meta_json,
        recorded_at,
    )?;
    advance_sessions(
        transaction,
        &commit.key.scope_json,
        &commit.session_keys,
        cursor,
    )?;
    publish_entity_versions(transaction, &commit.key, &commit.new_versions, cursor)?;
    Ok(EntityCommitOutcome::Committed)
}

fn advance_sessions(
    transaction: &rusqlite::Transaction<'_>,
    scope_json: &str,
    session_keys: &[String],
    cursor: i64,
) -> Result<(), ProviderError> {
    for session_key in session_keys {
        transaction
            .execute(
                "INSERT INTO patchouli_session_frontier (
                    scope_json, session_key_json, cursor
                 ) VALUES (?1, ?2, ?3)
                 ON CONFLICT (scope_json, session_key_json)
                 DO UPDATE SET cursor = max(cursor, excluded.cursor)",
                (scope_json, session_key, cursor),
            )
            .map_err(database_error)?;
    }
    Ok(())
}

fn stored_change_kind(kind: StoredChangeKind) -> &'static str {
    match kind {
        StoredChangeKind::Conflicted => "conflicted",
        StoredChangeKind::Created => "created",
        StoredChangeKind::Deleted => "deleted",
        StoredChangeKind::Resolved => "resolved",
        StoredChangeKind::Updated => "updated",
    }
}
