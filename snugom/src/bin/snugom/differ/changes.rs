//! Types for representing schema changes between versions.

use crate::scanner::{EntitySchema, FieldInfo, FilterableType, RelationInfo, UniqueConstraint};
use std::collections::HashMap;

/// Overall diff result for an entity
#[derive(Debug, Clone)]
pub struct EntityDiff {
    /// Entity name
    pub entity: String,
    /// Collection name
    pub collection: Option<String>,
    /// Previous schema version (None if new entity)
    pub old_version: Option<u32>,
    /// New schema version
    pub new_version: u32,
    /// Source file path
    pub source_file: String,
    /// All detected changes
    pub changes: Vec<EntityChange>,
    /// Overall migration complexity
    pub complexity: MigrationComplexity,
}

#[allow(dead_code)]
impl EntityDiff {
    /// Check if this is a new entity (no previous snapshot)
    pub fn is_new(&self) -> bool {
        self.old_version.is_none()
    }

    /// Check if there are any changes
    pub fn has_changes(&self) -> bool {
        !self.changes.is_empty() || self.is_new()
    }

    /// Get field changes only
    pub fn field_changes(&self) -> Vec<&FieldChange> {
        self.changes
            .iter()
            .filter_map(|c| match c {
                EntityChange::Field(fc) => Some(fc),
                _ => None,
            })
            .collect()
    }

    /// Get index changes only
    pub fn index_changes(&self) -> Vec<&IndexChange> {
        self.changes
            .iter()
            .filter_map(|c| match c {
                EntityChange::Index(ic) => Some(ic),
                _ => None,
            })
            .collect()
    }

    /// Get relation changes only
    pub fn relation_changes(&self) -> Vec<&RelationChange> {
        self.changes
            .iter()
            .filter_map(|c| match c {
                EntityChange::Relation(rc) => Some(rc),
                _ => None,
            })
            .collect()
    }
}

/// Individual change types
#[derive(Debug, Clone)]
pub enum EntityChange {
    Field(FieldChange),
    Index(IndexChange),
    Relation(RelationChange),
    UniqueConstraint(UniqueConstraintChange),
}

/// Field change details
#[derive(Debug, Clone)]
pub struct FieldChange {
    /// Field name
    pub name: String,
    /// Type of change
    pub change_type: ChangeType,
    /// Old field info (for modified/removed)
    pub old_field: Option<FieldInfo>,
    /// New field info (for added/modified)
    pub new_field: Option<FieldInfo>,
}

#[allow(dead_code)]
impl FieldChange {
    /// Create an "added field" change
    pub fn added(field: FieldInfo) -> Self {
        Self {
            name: field.name.clone(),
            change_type: ChangeType::Added,
            old_field: None,
            new_field: Some(field),
        }
    }

    /// Create a "removed field" change
    pub fn removed(field: FieldInfo) -> Self {
        Self {
            name: field.name.clone(),
            change_type: ChangeType::Removed,
            old_field: Some(field),
            new_field: None,
        }
    }

    /// Create a "modified field" change
    pub fn modified(old: FieldInfo, new: FieldInfo) -> Self {
        Self {
            name: new.name.clone(),
            change_type: ChangeType::Modified,
            old_field: Some(old),
            new_field: Some(new),
        }
    }

    /// Check if this is adding an optional field (auto-generatable)
    pub fn is_optional_addition(&self) -> bool {
        if self.change_type != ChangeType::Added {
            return false;
        }
        if let Some(ref field) = self.new_field {
            field.field_type.starts_with("Option<")
        } else {
            false
        }
    }

    /// Check if this is adding a Vec field (auto-generatable with empty array)
    pub fn is_vec_addition(&self) -> bool {
        if self.change_type != ChangeType::Added {
            return false;
        }
        if let Some(ref field) = self.new_field {
            field.field_type.starts_with("Vec<")
        } else {
            false
        }
    }

    /// Check if this is a type change (requires stub)
    pub fn is_type_change(&self) -> bool {
        if self.change_type != ChangeType::Modified {
            return false;
        }
        if let (Some(old), Some(new)) = (&self.old_field, &self.new_field) {
            old.field_type != new.field_type
        } else {
            false
        }
    }

    /// Check if field has serde default
    pub fn has_serde_default(&self) -> bool {
        if let Some(ref field) = self.new_field {
            field.serde_default.is_some()
        } else {
            false
        }
    }
}

/// Index change details
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct IndexChange {
    /// Field name
    pub field: String,
    /// Type of change
    pub change_type: ChangeType,
    /// Old index type (for modified/removed)
    pub old_type: Option<FilterableType>,
    /// New index type (for added/modified)
    pub new_type: Option<FilterableType>,
}

impl IndexChange {
    pub fn added(field: String, index_type: FilterableType) -> Self {
        Self {
            field,
            change_type: ChangeType::Added,
            old_type: None,
            new_type: Some(index_type),
        }
    }

    pub fn removed(field: String, index_type: FilterableType) -> Self {
        Self {
            field,
            change_type: ChangeType::Removed,
            old_type: Some(index_type),
            new_type: None,
        }
    }

    pub fn modified(field: String, old_type: FilterableType, new_type: FilterableType) -> Self {
        Self {
            field,
            change_type: ChangeType::Modified,
            old_type: Some(old_type),
            new_type: Some(new_type),
        }
    }
}

/// Relation change details
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct RelationChange {
    /// Field name
    pub field: String,
    /// Type of change
    pub change_type: ChangeType,
    /// Old relation info
    pub old_relation: Option<RelationInfo>,
    /// New relation info
    pub new_relation: Option<RelationInfo>,
}

impl RelationChange {
    pub fn added(relation: RelationInfo) -> Self {
        Self {
            field: relation.field.clone(),
            change_type: ChangeType::Added,
            old_relation: None,
            new_relation: Some(relation),
        }
    }

    pub fn removed(relation: RelationInfo) -> Self {
        Self {
            field: relation.field.clone(),
            change_type: ChangeType::Removed,
            old_relation: Some(relation),
            new_relation: None,
        }
    }

    pub fn modified(old: RelationInfo, new: RelationInfo) -> Self {
        Self {
            field: new.field.clone(),
            change_type: ChangeType::Modified,
            old_relation: Some(old),
            new_relation: Some(new),
        }
    }
}

/// Unique constraint change
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct UniqueConstraintChange {
    /// Fields involved
    pub fields: Vec<String>,
    /// Type of change
    pub change_type: ChangeType,
    /// Old constraint
    pub old_constraint: Option<UniqueConstraint>,
    /// New constraint
    pub new_constraint: Option<UniqueConstraint>,
}

impl UniqueConstraintChange {
    pub fn added(constraint: UniqueConstraint) -> Self {
        Self {
            fields: constraint.fields.clone(),
            change_type: ChangeType::Added,
            old_constraint: None,
            new_constraint: Some(constraint),
        }
    }

    pub fn removed(constraint: UniqueConstraint) -> Self {
        Self {
            fields: constraint.fields.clone(),
            change_type: ChangeType::Removed,
            old_constraint: Some(constraint),
            new_constraint: None,
        }
    }
}

/// Type of change
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    Added,
    Removed,
    Modified,
}

impl std::fmt::Display for ChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeType::Added => write!(f, "+"),
            ChangeType::Removed => write!(f, "-"),
            ChangeType::Modified => write!(f, "~"),
        }
    }
}

/// Migration complexity classification
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationComplexity {
    /// New entity, no migration needed (baseline)
    Baseline,
    /// All changes can be auto-generated
    Auto,
    /// Some changes require manual implementation (stub)
    Stub,
    /// Complex migration with full SnugOM access needed
    Complex,
    /// Metadata only, no document changes
    MetadataOnly,
}

impl std::fmt::Display for MigrationComplexity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MigrationComplexity::Baseline => write!(f, "BASELINE"),
            MigrationComplexity::Auto => write!(f, "AUTO"),
            MigrationComplexity::Stub => write!(f, "STUB"),
            MigrationComplexity::Complex => write!(f, "COMPLEX"),
            MigrationComplexity::MetadataOnly => write!(f, "METADATA-ONLY"),
        }
    }
}

/// Compare two entity schemas and produce a diff
pub fn diff_schemas(old: Option<&EntitySchema>, new: &EntitySchema) -> EntityDiff {
    let mut changes = Vec::new();

    // If no old schema, this is a new entity
    if old.is_none() {
        return EntityDiff {
            entity: new.entity.clone(),
            collection: new.collection.clone(),
            old_version: None,
            new_version: new.schema,
            source_file: new.source_file.clone(),
            changes: Vec::new(),
            complexity: MigrationComplexity::Baseline,
        };
    }

    let old = old.unwrap();

    // Build field maps for comparison
    let old_fields: HashMap<&str, &FieldInfo> = old.fields.iter().map(|f| (f.name.as_str(), f)).collect();
    let new_fields: HashMap<&str, &FieldInfo> = new.fields.iter().map(|f| (f.name.as_str(), f)).collect();

    // Find added fields
    for (name, new_field) in &new_fields {
        if !old_fields.contains_key(name) {
            changes.push(EntityChange::Field(FieldChange::added((*new_field).clone())));
        }
    }

    // Find removed fields
    for (name, old_field) in &old_fields {
        if !new_fields.contains_key(name) {
            changes.push(EntityChange::Field(FieldChange::removed((*old_field).clone())));
        }
    }

    // Find modified fields
    for (name, new_field) in &new_fields {
        if let Some(old_field) = old_fields.get(name)
            && fields_differ(old_field, new_field)
        {
            changes.push(EntityChange::Field(FieldChange::modified(
                (*old_field).clone(),
                (*new_field).clone(),
            )));
        }
    }

    // Compare indexes
    let old_indexes: HashMap<&str, FilterableType> = old
        .fields
        .iter()
        .filter_map(|f| f.filterable.map(|ft| (f.name.as_str(), ft)))
        .collect();
    let new_indexes: HashMap<&str, FilterableType> = new
        .fields
        .iter()
        .filter_map(|f| f.filterable.map(|ft| (f.name.as_str(), ft)))
        .collect();

    for (name, new_type) in &new_indexes {
        match old_indexes.get(name) {
            None => {
                changes.push(EntityChange::Index(IndexChange::added(name.to_string(), *new_type)));
            }
            Some(old_type) if old_type != new_type => {
                changes.push(EntityChange::Index(IndexChange::modified(
                    name.to_string(),
                    *old_type,
                    *new_type,
                )));
            }
            _ => {}
        }
    }

    for (name, old_type) in &old_indexes {
        if !new_indexes.contains_key(name) {
            changes.push(EntityChange::Index(IndexChange::removed(name.to_string(), *old_type)));
        }
    }

    // Compare relations
    let old_relations: HashMap<&str, &RelationInfo> = old.relations.iter().map(|r| (r.field.as_str(), r)).collect();
    let new_relations: HashMap<&str, &RelationInfo> = new.relations.iter().map(|r| (r.field.as_str(), r)).collect();

    for (name, new_rel) in &new_relations {
        match old_relations.get(name) {
            None => {
                changes.push(EntityChange::Relation(RelationChange::added((*new_rel).clone())));
            }
            Some(old_rel) if relations_differ(old_rel, new_rel) => {
                changes.push(EntityChange::Relation(RelationChange::modified(
                    (*old_rel).clone(),
                    (*new_rel).clone(),
                )));
            }
            _ => {}
        }
    }

    for (name, old_rel) in &old_relations {
        if !new_relations.contains_key(name) {
            changes.push(EntityChange::Relation(RelationChange::removed((*old_rel).clone())));
        }
    }

    // Compare unique constraints
    let old_uniques: Vec<&UniqueConstraint> = old.unique_constraints.iter().collect();
    let new_uniques: Vec<&UniqueConstraint> = new.unique_constraints.iter().collect();

    // Simple comparison by fields (could be improved)
    for new_uc in &new_uniques {
        let found = old_uniques.iter().any(|old_uc| old_uc.fields == new_uc.fields);
        if !found {
            changes.push(EntityChange::UniqueConstraint(UniqueConstraintChange::added((*new_uc).clone())));
        }
    }

    for old_uc in &old_uniques {
        let found = new_uniques.iter().any(|new_uc| new_uc.fields == old_uc.fields);
        if !found {
            changes.push(EntityChange::UniqueConstraint(UniqueConstraintChange::removed(
                (*old_uc).clone(),
            )));
        }
    }

    // Also check single-field unique changes
    for (name, new_field) in &new_fields {
        if let Some(old_field) = old_fields.get(name) {
            // Unique added
            if !old_field.unique && new_field.unique {
                let constraint = UniqueConstraint {
                    fields: vec![new_field.name.clone()],
                    case_insensitive: new_field.unique_case_insensitive,
                };
                changes.push(EntityChange::UniqueConstraint(UniqueConstraintChange::added(constraint)));
            }
            // Unique removed
            if old_field.unique && !new_field.unique {
                let constraint = UniqueConstraint {
                    fields: vec![old_field.name.clone()],
                    case_insensitive: old_field.unique_case_insensitive,
                };
                changes.push(EntityChange::UniqueConstraint(UniqueConstraintChange::removed(constraint)));
            }
        }
    }

    // Classify complexity
    let complexity = classify_complexity(&changes);

    EntityDiff {
        entity: new.entity.clone(),
        collection: new.collection.clone(),
        old_version: Some(old.schema),
        new_version: old.schema + 1, // Increment version
        source_file: new.source_file.clone(),
        changes,
        complexity,
    }
}

/// Check if two fields differ in a meaningful way
fn fields_differ(old: &FieldInfo, new: &FieldInfo) -> bool {
    old.field_type != new.field_type
        || old.id != new.id
        || old.filterable != new.filterable
        || old.sortable != new.sortable
        || old.unique != new.unique
        || old.unique_case_insensitive != new.unique_case_insensitive
        || old.datetime_format != new.datetime_format
}

/// Check if two relations differ
fn relations_differ(old: &RelationInfo, new: &RelationInfo) -> bool {
    old.target != new.target || old.kind != new.kind || old.cascade != new.cascade
}

/// Classify the overall migration complexity
fn classify_complexity(changes: &[EntityChange]) -> MigrationComplexity {
    if changes.is_empty() {
        return MigrationComplexity::MetadataOnly;
    }

    let mut needs_stub = false;
    let mut needs_doc_changes = false;

    for change in changes {
        match change {
            EntityChange::Field(fc) => {
                match fc.change_type {
                    ChangeType::Added => {
                        needs_doc_changes = true;
                        // Non-optional, non-default fields need stub
                        if let Some(ref field) = fc.new_field
                            && !field.field_type.starts_with("Option<")
                            && !field.field_type.starts_with("Vec<")
                            && fc.new_field.as_ref().and_then(|f| f.serde_default.as_ref()).is_none()
                        {
                            // Check if it's a primitive with a sensible default
                            let ty = &field.field_type;
                            if !is_defaultable_type(ty) {
                                needs_stub = true;
                            }
                        }
                    }
                    ChangeType::Removed => {
                        needs_doc_changes = true;
                    }
                    ChangeType::Modified => {
                        // Type changes require stub
                        if fc.is_type_change() {
                            needs_stub = true;
                            needs_doc_changes = true;
                        }
                    }
                }
            }
            EntityChange::Index(_) => {
                // Index changes don't require doc changes, just rebuild
            }
            EntityChange::Relation(rc) => {
                // has_many removals are metadata only
                // belongs_to additions with new field need doc changes
                if rc.change_type == ChangeType::Added {
                    // If the field was also added, it's handled by field changes
                }
            }
            EntityChange::UniqueConstraint(uc) => {
                // Adding unique constraint needs validation
                if uc.change_type == ChangeType::Added {
                    // Validation is auto but could fail
                }
            }
        }
    }

    if needs_stub {
        MigrationComplexity::Stub
    } else if needs_doc_changes {
        MigrationComplexity::Auto
    } else {
        MigrationComplexity::MetadataOnly
    }
}

/// Check if a type has a sensible default value we can generate
fn is_defaultable_type(ty: &str) -> bool {
    matches!(
        ty,
        "String"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "f32"
            | "f64"
            | "bool"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{CascadeStrategy, FilterableType, RelationKind, UniqueConstraint};
    use chrono::Utc;

    fn make_schema(name: &str, version: u32, fields: Vec<FieldInfo>) -> EntitySchema {
        EntitySchema {
            entity: name.to_string(),
            collection: Some(name.to_lowercase()),
            schema: version,
            fields,
            relations: Vec::new(),
            unique_constraints: Vec::new(),
            indexes: Vec::new(),
            generated_at: Utc::now(),
            source_file: "test.rs".to_string(),
            source_line: 1,
        }
    }

    #[test]
    fn test_new_entity_is_baseline() {
        let new = make_schema("User", 1, vec![FieldInfo::new("id".to_string(), "String".to_string())]);
        let diff = diff_schemas(None, &new);

        assert!(diff.is_new());
        assert_eq!(diff.complexity, MigrationComplexity::Baseline);
    }

    #[test]
    fn test_detect_added_field() {
        let old = make_schema("User", 1, vec![FieldInfo::new("id".to_string(), "String".to_string())]);
        let new = make_schema(
            "User",
            1,
            vec![
                FieldInfo::new("id".to_string(), "String".to_string()),
                FieldInfo::new("name".to_string(), "String".to_string()),
            ],
        );

        let diff = diff_schemas(Some(&old), &new);
        assert!(diff.has_changes());
        assert_eq!(diff.field_changes().len(), 1);
        assert_eq!(diff.field_changes()[0].change_type, ChangeType::Added);
        assert_eq!(diff.field_changes()[0].name, "name");
    }

    #[test]
    fn test_detect_removed_field() {
        let old = make_schema(
            "User",
            1,
            vec![
                FieldInfo::new("id".to_string(), "String".to_string()),
                FieldInfo::new("legacy".to_string(), "String".to_string()),
            ],
        );
        let new = make_schema("User", 1, vec![FieldInfo::new("id".to_string(), "String".to_string())]);

        let diff = diff_schemas(Some(&old), &new);
        assert!(diff.has_changes());
        assert_eq!(diff.field_changes().len(), 1);
        assert_eq!(diff.field_changes()[0].change_type, ChangeType::Removed);
        assert_eq!(diff.field_changes()[0].name, "legacy");
    }

    #[test]
    fn test_optional_field_is_auto() {
        let old = make_schema("User", 1, vec![FieldInfo::new("id".to_string(), "String".to_string())]);
        let new = make_schema(
            "User",
            1,
            vec![
                FieldInfo::new("id".to_string(), "String".to_string()),
                FieldInfo::new("bio".to_string(), "Option<String>".to_string()),
            ],
        );

        let diff = diff_schemas(Some(&old), &new);
        assert_eq!(diff.complexity, MigrationComplexity::Auto);
        assert!(diff.field_changes()[0].is_optional_addition());
    }

    #[test]
    fn test_no_changes_is_metadata_only() {
        let old = make_schema("User", 1, vec![FieldInfo::new("id".to_string(), "String".to_string())]);
        let new = make_schema("User", 1, vec![FieldInfo::new("id".to_string(), "String".to_string())]);

        let diff = diff_schemas(Some(&old), &new);
        assert!(!diff.has_changes());
        assert_eq!(diff.complexity, MigrationComplexity::MetadataOnly);
    }

    #[test]
    fn test_type_change_is_stub() {
        let old = make_schema(
            "User",
            1,
            vec![
                FieldInfo::new("id".to_string(), "String".to_string()),
                FieldInfo::new("age".to_string(), "String".to_string()),
            ],
        );
        let new = make_schema(
            "User",
            1,
            vec![
                FieldInfo::new("id".to_string(), "String".to_string()),
                FieldInfo::new("age".to_string(), "i32".to_string()),
            ],
        );

        let diff = diff_schemas(Some(&old), &new);
        assert!(diff.has_changes());
        assert_eq!(diff.complexity, MigrationComplexity::Stub);

        // Should show as Modified
        let field_changes = diff.field_changes();
        assert_eq!(field_changes.len(), 1);
        assert_eq!(field_changes[0].change_type, ChangeType::Modified);
        assert_eq!(field_changes[0].name, "age");
    }

    #[test]
    fn test_required_string_field_addition_is_auto() {
        // String is defaultable (to ""), so adding a required String field is Auto
        let old = make_schema("User", 1, vec![FieldInfo::new("id".to_string(), "String".to_string())]);
        let new = make_schema(
            "User",
            1,
            vec![
                FieldInfo::new("id".to_string(), "String".to_string()),
                FieldInfo::new("email".to_string(), "String".to_string()),
            ],
        );

        let diff = diff_schemas(Some(&old), &new);
        assert!(diff.has_changes());
        // String is defaultable to "", so this is Auto
        assert_eq!(diff.complexity, MigrationComplexity::Auto);
    }

    #[test]
    fn test_required_custom_type_field_addition_is_stub() {
        // Custom types (not defaultable) require stub
        let old = make_schema("User", 1, vec![FieldInfo::new("id".to_string(), "String".to_string())]);
        let new = make_schema(
            "User",
            1,
            vec![
                FieldInfo::new("id".to_string(), "String".to_string()),
                FieldInfo::new("config".to_string(), "UserConfig".to_string()), // Custom type
            ],
        );

        let diff = diff_schemas(Some(&old), &new);
        assert!(diff.has_changes());
        // Custom type with no default needs manual intervention
        assert_eq!(diff.complexity, MigrationComplexity::Stub);
    }

    #[test]
    fn test_defaultable_required_field_is_auto() {
        let old = make_schema("User", 1, vec![FieldInfo::new("id".to_string(), "String".to_string())]);
        let new = make_schema(
            "User",
            1,
            vec![
                FieldInfo::new("id".to_string(), "String".to_string()),
                FieldInfo::new("count".to_string(), "i32".to_string()), // Defaultable to 0
            ],
        );

        let diff = diff_schemas(Some(&old), &new);
        assert!(diff.has_changes());
        assert_eq!(diff.complexity, MigrationComplexity::Auto);
    }

    #[test]
    fn test_vec_field_addition_is_auto() {
        let old = make_schema("User", 1, vec![FieldInfo::new("id".to_string(), "String".to_string())]);
        let new = make_schema(
            "User",
            1,
            vec![
                FieldInfo::new("id".to_string(), "String".to_string()),
                FieldInfo::new("tags".to_string(), "Vec<String>".to_string()), // Defaults to []
            ],
        );

        let diff = diff_schemas(Some(&old), &new);
        assert!(diff.has_changes());
        assert_eq!(diff.complexity, MigrationComplexity::Auto);
    }

    fn make_field_with_filterable(name: &str, field_type: &str, filterable: Option<FilterableType>) -> FieldInfo {
        let mut field = FieldInfo::new(name.to_string(), field_type.to_string());
        field.filterable = filterable;
        field
    }

    #[test]
    fn test_index_added() {
        // Index changes are detected via field.filterable, not the indexes array
        let old = make_schema(
            "User",
            1,
            vec![
                FieldInfo::new("id".to_string(), "String".to_string()),
                FieldInfo::new("email".to_string(), "String".to_string()), // No filterable
            ],
        );
        let new = make_schema(
            "User",
            1,
            vec![
                FieldInfo::new("id".to_string(), "String".to_string()),
                make_field_with_filterable("email", "String", Some(FilterableType::Tag)),
            ],
        );

        let diff = diff_schemas(Some(&old), &new);
        assert!(diff.has_changes());

        let index_changes: Vec<_> = diff
            .changes
            .iter()
            .filter_map(|c| match c {
                EntityChange::Index(ic) => Some(ic),
                _ => None,
            })
            .collect();
        assert_eq!(index_changes.len(), 1);
        assert_eq!(index_changes[0].change_type, ChangeType::Added);
        assert_eq!(index_changes[0].field, "email");
    }

    #[test]
    fn test_index_removed() {
        let old = make_schema(
            "User",
            1,
            vec![
                FieldInfo::new("id".to_string(), "String".to_string()),
                make_field_with_filterable("email", "String", Some(FilterableType::Tag)),
            ],
        );
        let new = make_schema(
            "User",
            1,
            vec![
                FieldInfo::new("id".to_string(), "String".to_string()),
                FieldInfo::new("email".to_string(), "String".to_string()), // No filterable
            ],
        );

        let diff = diff_schemas(Some(&old), &new);
        assert!(diff.has_changes());

        let index_changes: Vec<_> = diff
            .changes
            .iter()
            .filter_map(|c| match c {
                EntityChange::Index(ic) => Some(ic),
                _ => None,
            })
            .collect();
        assert_eq!(index_changes.len(), 1);
        assert_eq!(index_changes[0].change_type, ChangeType::Removed);
    }

    #[test]
    fn test_index_type_changed() {
        let old = make_schema(
            "User",
            1,
            vec![
                FieldInfo::new("id".to_string(), "String".to_string()),
                make_field_with_filterable("name", "String", Some(FilterableType::Tag)),
            ],
        );
        let new = make_schema(
            "User",
            1,
            vec![
                FieldInfo::new("id".to_string(), "String".to_string()),
                make_field_with_filterable("name", "String", Some(FilterableType::Text)), // Changed
            ],
        );

        let diff = diff_schemas(Some(&old), &new);
        assert!(diff.has_changes());

        let index_changes: Vec<_> = diff
            .changes
            .iter()
            .filter_map(|c| match c {
                EntityChange::Index(ic) => Some(ic),
                _ => None,
            })
            .collect();
        assert_eq!(index_changes.len(), 1);
        assert_eq!(index_changes[0].change_type, ChangeType::Modified);
    }

    fn make_schema_with_relations(
        name: &str,
        version: u32,
        fields: Vec<FieldInfo>,
        relations: Vec<RelationInfo>,
    ) -> EntitySchema {
        EntitySchema {
            entity: name.to_string(),
            collection: Some(name.to_lowercase()),
            schema: version,
            fields,
            relations,
            unique_constraints: Vec::new(),
            indexes: Vec::new(),
            generated_at: Utc::now(),
            source_file: "test.rs".to_string(),
            source_line: 1,
        }
    }

    #[test]
    fn test_relation_added() {
        let old =
            make_schema_with_relations("Post", 1, vec![FieldInfo::new("id".to_string(), "String".to_string())], vec![]);
        let new = make_schema_with_relations(
            "Post",
            1,
            vec![FieldInfo::new("id".to_string(), "String".to_string())],
            vec![RelationInfo {
                field: "author_id".to_string(),
                target: "users".to_string(),
                kind: RelationKind::BelongsTo,
                cascade: CascadeStrategy::Detach,
            }],
        );

        let diff = diff_schemas(Some(&old), &new);
        assert!(diff.has_changes());

        let relation_changes: Vec<_> = diff
            .changes
            .iter()
            .filter_map(|c| match c {
                EntityChange::Relation(rc) => Some(rc),
                _ => None,
            })
            .collect();
        assert_eq!(relation_changes.len(), 1);
        assert_eq!(relation_changes[0].change_type, ChangeType::Added);
        assert_eq!(relation_changes[0].field, "author_id");
    }

    #[test]
    fn test_relation_removed() {
        let old = make_schema_with_relations(
            "Post",
            1,
            vec![FieldInfo::new("id".to_string(), "String".to_string())],
            vec![RelationInfo {
                field: "author_id".to_string(),
                target: "users".to_string(),
                kind: RelationKind::BelongsTo,
                cascade: CascadeStrategy::Detach,
            }],
        );
        let new =
            make_schema_with_relations("Post", 1, vec![FieldInfo::new("id".to_string(), "String".to_string())], vec![]);

        let diff = diff_schemas(Some(&old), &new);
        assert!(diff.has_changes());

        let relation_changes: Vec<_> = diff
            .changes
            .iter()
            .filter_map(|c| match c {
                EntityChange::Relation(rc) => Some(rc),
                _ => None,
            })
            .collect();
        assert_eq!(relation_changes.len(), 1);
        assert_eq!(relation_changes[0].change_type, ChangeType::Removed);
    }

    #[test]
    fn test_relation_target_changed() {
        let old = make_schema_with_relations(
            "Post",
            1,
            vec![FieldInfo::new("id".to_string(), "String".to_string())],
            vec![RelationInfo {
                field: "owner_id".to_string(),
                target: "users".to_string(),
                kind: RelationKind::BelongsTo,
                cascade: CascadeStrategy::Detach,
            }],
        );
        let new = make_schema_with_relations(
            "Post",
            1,
            vec![FieldInfo::new("id".to_string(), "String".to_string())],
            vec![RelationInfo {
                field: "owner_id".to_string(),
                target: "organizations".to_string(), // Changed target
                kind: RelationKind::BelongsTo,
                cascade: CascadeStrategy::Detach,
            }],
        );

        let diff = diff_schemas(Some(&old), &new);
        assert!(diff.has_changes());

        let relation_changes: Vec<_> = diff
            .changes
            .iter()
            .filter_map(|c| match c {
                EntityChange::Relation(rc) => Some(rc),
                _ => None,
            })
            .collect();
        assert_eq!(relation_changes.len(), 1);
        assert_eq!(relation_changes[0].change_type, ChangeType::Modified);
    }

    fn make_schema_with_constraints(
        name: &str,
        version: u32,
        fields: Vec<FieldInfo>,
        unique_constraints: Vec<UniqueConstraint>,
    ) -> EntitySchema {
        EntitySchema {
            entity: name.to_string(),
            collection: Some(name.to_lowercase()),
            schema: version,
            fields,
            relations: Vec::new(),
            unique_constraints,
            indexes: Vec::new(),
            generated_at: Utc::now(),
            source_file: "test.rs".to_string(),
            source_line: 1,
        }
    }

    #[test]
    fn test_unique_constraint_added() {
        let old = make_schema_with_constraints(
            "User",
            1,
            vec![FieldInfo::new("id".to_string(), "String".to_string())],
            vec![],
        );
        let new = make_schema_with_constraints(
            "User",
            1,
            vec![FieldInfo::new("id".to_string(), "String".to_string())],
            vec![UniqueConstraint {
                fields: vec!["email".to_string()],
                case_insensitive: false,
            }],
        );

        let diff = diff_schemas(Some(&old), &new);
        assert!(diff.has_changes());

        let constraint_changes: Vec<_> = diff
            .changes
            .iter()
            .filter_map(|c| match c {
                EntityChange::UniqueConstraint(uc) => Some(uc),
                _ => None,
            })
            .collect();
        assert_eq!(constraint_changes.len(), 1);
        assert_eq!(constraint_changes[0].change_type, ChangeType::Added);
    }

    #[test]
    fn test_unique_constraint_removed() {
        let old = make_schema_with_constraints(
            "User",
            1,
            vec![FieldInfo::new("id".to_string(), "String".to_string())],
            vec![UniqueConstraint {
                fields: vec!["email".to_string()],
                case_insensitive: false,
            }],
        );
        let new = make_schema_with_constraints(
            "User",
            1,
            vec![FieldInfo::new("id".to_string(), "String".to_string())],
            vec![],
        );

        let diff = diff_schemas(Some(&old), &new);
        assert!(diff.has_changes());

        let constraint_changes: Vec<_> = diff
            .changes
            .iter()
            .filter_map(|c| match c {
                EntityChange::UniqueConstraint(uc) => Some(uc),
                _ => None,
            })
            .collect();
        assert_eq!(constraint_changes.len(), 1);
        assert_eq!(constraint_changes[0].change_type, ChangeType::Removed);
    }

    #[test]
    fn test_compound_unique_constraint() {
        let old = make_schema_with_constraints(
            "User",
            1,
            vec![FieldInfo::new("id".to_string(), "String".to_string())],
            vec![],
        );
        let new = make_schema_with_constraints(
            "User",
            1,
            vec![FieldInfo::new("id".to_string(), "String".to_string())],
            vec![UniqueConstraint {
                fields: vec!["tenant_id".to_string(), "email".to_string()],
                case_insensitive: true,
            }],
        );

        let diff = diff_schemas(Some(&old), &new);
        assert!(diff.has_changes());

        let constraint_changes: Vec<_> = diff
            .changes
            .iter()
            .filter_map(|c| match c {
                EntityChange::UniqueConstraint(uc) => Some(uc),
                _ => None,
            })
            .collect();
        assert_eq!(constraint_changes.len(), 1);
        assert_eq!(constraint_changes[0].fields, vec!["tenant_id", "email"]);
    }

    #[test]
    fn test_multiple_changes_combined() {
        let old = make_schema(
            "User",
            1,
            vec![
                FieldInfo::new("id".to_string(), "String".to_string()),
                FieldInfo::new("legacy_field".to_string(), "String".to_string()),
            ],
        );

        // Use filterable on fields to trigger index detection
        let new_schema = EntitySchema {
            entity: "User".to_string(),
            collection: Some("user".to_string()),
            schema: 1,
            fields: vec![
                FieldInfo::new("id".to_string(), "String".to_string()),
                make_field_with_filterable("email", "Option<String>", Some(FilterableType::Tag)), // Added with index
            ], // legacy_field removed
            relations: vec![RelationInfo {
                field: "org_id".to_string(),
                target: "organizations".to_string(),
                kind: RelationKind::BelongsTo,
                cascade: CascadeStrategy::Detach,
            }],
            unique_constraints: vec![],
            indexes: vec![],
            generated_at: Utc::now(),
            source_file: "test.rs".to_string(),
            source_line: 1,
        };

        let diff = diff_schemas(Some(&old), &new_schema);
        assert!(diff.has_changes());

        // Should have: 1 field added, 1 field removed, 1 relation added, 1 index added
        // Note: field added and index added are both for "email"
        assert!(
            diff.changes.len() >= 3,
            "Expected at least 3 changes, got {}",
            diff.changes.len()
        );
    }

    #[test]
    fn test_field_removed_is_auto() {
        // Field removal is classified as Auto (data can be removed automatically)
        // Only type changes or non-defaultable additions are Stub
        let old = make_schema(
            "User",
            1,
            vec![
                FieldInfo::new("id".to_string(), "String".to_string()),
                FieldInfo::new("deprecated_field".to_string(), "String".to_string()),
            ],
        );
        let new = make_schema("User", 1, vec![FieldInfo::new("id".to_string(), "String".to_string())]);

        let diff = diff_schemas(Some(&old), &new);
        assert!(diff.has_changes());
        assert_eq!(diff.field_changes().len(), 1);
        assert_eq!(diff.field_changes()[0].change_type, ChangeType::Removed);
        // Field removal is Auto (can be done automatically)
        assert_eq!(diff.complexity, MigrationComplexity::Auto);
    }
}
