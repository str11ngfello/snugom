//! Diff detection module for comparing current entity schemas to snapshots.
//!
//! This module provides functionality to:
//! - Load existing schema snapshots from disk
//! - Compare current entity schemas to snapshots
//! - Detect added, removed, and changed fields
//! - Classify migration complexity (auto vs stub)

mod changes;
mod loader;

#[allow(unused_imports)]
pub use changes::{
    diff_schemas, ChangeType, EntityChange, EntityDiff, FieldChange, IndexChange,
    MigrationComplexity, RelationChange, UniqueConstraintChange,
};
#[allow(unused_imports)]
pub use loader::load_latest_snapshots;
