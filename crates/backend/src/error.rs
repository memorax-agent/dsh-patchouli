use thiserror::Error;

use crate::VersionToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendErrorReason {
    Cancelled,
    CursorExpired,
    DeadlineExceeded,
    IdempotencyConflict,
    NotFound,
    Overloaded,
    UnsupportedCapability,
    VersionConflict,
}

#[derive(Debug, Error)]
#[error("{reason:?}: {message}")]
pub struct BackendError {
    pub reason: BackendErrorReason,
    pub message: String,
    pub current_versions: Vec<VersionToken>,
}

impl BackendError {
    pub fn new(reason: BackendErrorReason, message: impl Into<String>) -> Self {
        Self {
            reason,
            message: message.into(),
            current_versions: Vec::new(),
        }
    }

    pub fn version_conflict(current_versions: Vec<VersionToken>) -> Self {
        Self {
            reason: BackendErrorReason::VersionConflict,
            message: "base_versions do not match the current heads".to_owned(),
            current_versions,
        }
    }
}
