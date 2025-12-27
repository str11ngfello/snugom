use std::borrow::Cow;

use thiserror::Error;

/// Top-level error type returned by SnugOM repositories.
#[derive(Debug, Error)]
pub enum RepoError {
    /// Validation failed for one or more fields.
    #[error("validation failed")]
    Validation(#[from] ValidationError),

    /// Underlying Redis command failed.
    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),

    /// Optimistic concurrency guard detected a stale version.
    #[error("version conflict (expected {expected:?}, actual {actual:?})")]
    VersionConflict { expected: Option<u64>, actual: Option<u64> },

    /// Target entity was not found when performing a mutation.
    #[error("entity not found")]
    NotFound { entity_id: Option<String> },

    /// Invalid input supplied to a repository/search operation.
    #[error("invalid request: {message}")]
    InvalidRequest { message: String },

    /// Unique constraint violation - the value(s) already exist on another entity.
    #[error("unique constraint violation: fields {fields:?} with values {values:?} already exist on entity '{existing_entity_id}'")]
    UniqueConstraintViolation {
        fields: Vec<String>,
        values: Vec<String>,
        existing_entity_id: String,
    },

    /// Placeholder for other error kinds while the crate is scaffolded.
    #[error("{message}")]
    Other { message: Cow<'static, str> },
}

/// Collection of validation issues encountered while preparing a mutation.
#[derive(Debug, Error)]
#[error("validation errors: {issues:?}")]
pub struct ValidationError {
    pub issues: Vec<ValidationIssue>,
}

impl ValidationError {
    pub fn new<I>(issues: I) -> Self
    where
        I: IntoIterator<Item = ValidationIssue>,
    {
        Self {
            issues: issues.into_iter().collect(),
        }
    }

    /// Convenience helper for constructing a single-field validation error.
    pub fn single(field: impl Into<String>, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new([ValidationIssue::new(field, code, message)])
    }

    pub fn is_empty(&self) -> bool {
        self.issues.is_empty()
    }
}

/// Detailed validation failure for a single field or logical path.
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub field: String,
    pub code: String,
    pub message: String,
}

impl ValidationIssue {
    pub fn new(field: impl Into<String>, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            code: code.into(),
            message: message.into(),
        }
    }
}

/// Convenience alias used by later phases when validation passed.
pub type ValidationResult<T> = Result<T, ValidationError>;
