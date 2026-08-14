use thiserror::Error;

use crate::{EntityRef, VersionToken};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityVersionConflict {
    pub entity_ref: EntityRef,
    pub current_versions: Vec<VersionToken>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendErrorReason {
    CursorExpired,
    DeadlineExceeded,
    IdempotencyConflict,
    InvalidRequest,
    NotFound,
    Overloaded,
    UnsupportedCapability,
    VersionConflict,
    WorkUnitExpired,
}

#[derive(Debug, Error)]
#[error("{reason:?}: {message}")]
pub struct BackendError {
    pub reason: BackendErrorReason,
    pub message: String,
    pub current_versions: Vec<VersionToken>,
    pub conflicts: Vec<EntityVersionConflict>,
}

impl BackendError {
    pub fn new(reason: BackendErrorReason, message: impl Into<String>) -> Self {
        Self {
            reason,
            message: message.into(),
            current_versions: Vec::new(),
            conflicts: Vec::new(),
        }
    }

    pub fn version_conflict(current_versions: Vec<VersionToken>) -> Self {
        Self {
            reason: BackendErrorReason::VersionConflict,
            message: "base_versions do not match the current heads".to_owned(),
            current_versions,
            conflicts: Vec::new(),
        }
    }

    pub fn entity_conflicts(conflicts: Vec<EntityVersionConflict>) -> Self {
        Self {
            reason: BackendErrorReason::VersionConflict,
            message: "one or more transaction entities conflict with current heads".to_owned(),
            current_versions: Vec::new(),
            conflicts,
        }
    }
}
