use crate::{
    errors::{ValidationError, ValidationResult},
    types::{DatetimeMirrorValue, EntityDescriptor},
};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationCommand {
    UpsertEntity(EntityMutation),
    PatchEntity(EntityPatch),
    DeleteEntity(EntityDelete),
    MutateRelations(RelationMutation),
    Upsert(UpsertCommand),
    GetOrCreate(GetOrCreateCommand),
}

/// Upsert command - creates if not exists, updates if exists.
/// Executed in a single Lua script to avoid race conditions.
#[derive(Debug, Serialize)]
pub struct UpsertCommand {
    /// Key to check for existence (from update clause)
    pub update_key: String,
    /// Entity ID from update clause (used for existence check)
    pub update_entity_id: String,
    /// Key for create path (may differ from update_key)
    pub create_key: String,
    /// Entity ID for create path
    pub create_entity_id: String,
    /// Full JSON payload for the create path
    pub create_payload_json: String,
    /// Unique constraints to enforce on create
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub create_unique_constraints: Vec<UniqueConstraintCheck>,
    /// Relations to establish on create
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub create_relations: Vec<RelationMutation>,
    /// Datetime mirror fields for create
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub datetime_mirrors: Vec<DatetimeMirrorValue>,
    /// Patch operations for the update path
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub update_operations: Vec<PatchOperationPayload>,
    /// Unique constraints to enforce on update
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub update_unique_constraints: Vec<UniqueConstraintCheck>,
    /// Relations to mutate on update
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub update_relations: Vec<RelationMutation>,
    /// Idempotency key for deduplication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    /// TTL for idempotency key in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_ttl: Option<u64>,
}

/// GetOrCreate command - returns existing entity or creates new one.
/// Executed in a single Lua script to avoid race conditions.
/// Unlike upsert, this does NOT update the entity if it exists.
#[derive(Debug, Serialize)]
pub struct GetOrCreateCommand {
    /// Key to check for existence and create at
    pub entity_key: String,
    /// Entity ID
    pub entity_id: String,
    /// Full JSON payload for the create path
    pub create_payload_json: String,
    /// Unique constraints to enforce on create
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unique_constraints: Vec<UniqueConstraintCheck>,
    /// Relations to establish on create
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<RelationMutation>,
    /// Datetime mirror fields for create
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub datetime_mirrors: Vec<DatetimeMirrorValue>,
    /// Idempotency key for deduplication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    /// TTL for idempotency key in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_ttl: Option<u64>,
}

/// Represents a unique constraint check to be enforced by the Lua script.
#[derive(Debug, Clone, Serialize)]
pub struct UniqueConstraintCheck {
    /// Field names that make up the constraint (single or compound)
    pub fields: Vec<String>,
    /// Whether string comparisons should be case-insensitive
    pub case_insensitive: bool,
    /// The values to check, in the same order as fields
    pub values: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct EntityMutation {
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<u64>,
    pub payload_json: String,
    pub entity_id: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub datetime_mirrors: Vec<DatetimeMirrorValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_ttl: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<RelationMutation>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unique_constraints: Vec<UniqueConstraintCheck>,
}

#[derive(Debug, Serialize)]
pub struct PatchOperationPayload {
    pub path: String,
    #[serde(rename = "type")]
    pub op_type: PatchOperationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirror: Option<DatetimeMirrorValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirror_value_json: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchOperationType {
    Assign,
    Merge,
    Delete,
}

#[derive(Debug, Serialize)]
pub struct EntityPatch {
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<u64>,
    pub operations: Vec<PatchOperationPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_ttl: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<RelationMutation>,
    /// Unique constraints that need to be enforced when updating unique fields.
    /// This contains the constraint definition plus the NEW values from the patch.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unique_constraints: Vec<UniqueConstraintCheck>,
}

#[derive(Debug, Serialize)]
pub struct EntityDelete {
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<DeleteCascadeRelation>,
    /// Unique constraint definitions for cleanup during delete.
    /// Unlike create, we only need field names and case_insensitive - values are read from the entity.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unique_constraints: Vec<UniqueConstraintDefinition>,
}

/// Represents a unique constraint definition for delete cleanup.
/// Values are read from the entity in Lua, not passed in.
#[derive(Debug, Clone, Serialize)]
pub struct UniqueConstraintDefinition {
    pub fields: Vec<String>,
    pub case_insensitive: bool,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum CascadeDirective {
    DeleteDependents,
    DetachDependents,
}

#[derive(Debug, Serialize, Clone)]
pub struct CascadeRelationSpec {
    pub alias: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_collection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_service: Option<String>,
    pub cascade: CascadeDirective,
    #[serde(skip_serializing_if = "skip_false")]
    pub maintain_reverse: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub child_relations: Vec<CascadeRelationSpec>,
}

#[derive(Debug, Serialize, Clone)]
pub struct DeleteCascadeRelation {
    pub alias: String,
    pub relation_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_collection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_service: Option<String>,
    pub cascade: CascadeDirective,
    #[serde(skip_serializing_if = "skip_false")]
    pub maintain_reverse: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub child_relations: Vec<CascadeRelationSpec>,
}

#[derive(Debug, Serialize, Clone)]
pub struct RelationMutation {
    pub relation_key: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub add: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cascade: Option<CascadeDirective>,
    #[serde(skip_serializing_if = "skip_false")]
    pub maintain_reverse: bool,
}

#[derive(Debug, Serialize, Default)]
pub struct MutationPlan {
    pub commands: Vec<MutationCommand>,
}

impl MutationPlan {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, command: MutationCommand) {
        self.commands.push(command);
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

pub fn build_entity_mutation(
    descriptor: &EntityDescriptor,
    key: String,
    payload: serde_json::Value,
    mirrors: Vec<DatetimeMirrorValue>,
    expected_version: Option<u64>,
    idempotency_key: Option<String>,
    idempotency_ttl: Option<u64>,
    relation_mutations: Vec<RelationMutation>,
) -> ValidationResult<EntityMutation> {
    let mut datetime_mirrors = mirrors;

    let payload_json = serde_json::to_string(&payload).map_err(|err| {
        ValidationError::single("payload", "serialization_error", format!("failed to serialize payload: {err}"))
    })?;

    let id_field = descriptor
        .id_field
        .as_ref()
        .ok_or_else(|| ValidationError::single("id", "missing", "entity id field is not defined"))?;

    let entity_id = payload
        .get(id_field)
        .and_then(|value| value.as_str())
        .ok_or_else(|| ValidationError::single(id_field.clone(), "missing", "entity id must be present"))?
        .to_string();

    if datetime_mirrors.is_empty() && !descriptor.fields.is_empty() {
        for field in &descriptor.fields {
            if let Some(mirror) = &field.datetime_mirror {
                datetime_mirrors.push(DatetimeMirrorValue::new(&field.name, mirror, None));
            }
        }
    }

    // Build unique constraint checks from descriptor
    let unique_constraints = build_unique_constraint_checks(descriptor, &payload);

    Ok(EntityMutation {
        key,
        expected_version,
        payload_json,
        entity_id,
        datetime_mirrors,
        idempotency_key,
        idempotency_ttl,
        relations: relation_mutations,
        unique_constraints,
    })
}

/// Extracts values for unique constraint fields from the payload.
pub fn build_unique_constraint_checks(
    descriptor: &EntityDescriptor,
    payload: &serde_json::Value,
) -> Vec<UniqueConstraintCheck> {
    let mut checks = Vec::new();

    for constraint in &descriptor.unique_constraints {
        let mut values = Vec::with_capacity(constraint.fields.len());
        for field_name in &constraint.fields {
            let value = payload
                .get(field_name)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            values.push(value);
        }

        checks.push(UniqueConstraintCheck {
            fields: constraint.fields.clone(),
            case_insensitive: constraint.case_insensitive,
            values,
        });
    }

    checks
}

pub fn build_entity_delete(
    key: String,
    expected_version: Option<u64>,
    relations: Vec<DeleteCascadeRelation>,
    unique_constraints: Vec<UniqueConstraintDefinition>,
) -> EntityDelete {
    EntityDelete {
        key,
        expected_version,
        relations,
        unique_constraints,
    }
}

pub fn build_entity_patch(
    key: String,
    entity_id: Option<String>,
    expected_version: Option<u64>,
    operations: Vec<crate::repository::PatchOperation>,
    idempotency_key: Option<String>,
    idempotency_ttl: Option<u64>,
    relation_mutations: Vec<RelationMutation>,
    unique_constraints: Vec<UniqueConstraintCheck>,
) -> EntityPatch {
    let ops = operations
        .into_iter()
        .map(|operation| {
            use crate::repository::PatchOpKind;
            let (op_type, value) = match operation.kind {
                PatchOpKind::Assign(value) => (PatchOperationType::Assign, Some(value)),
                PatchOpKind::Merge(value) => (PatchOperationType::Merge, Some(value)),
                PatchOpKind::Delete => (PatchOperationType::Delete, None),
            };
            let value_json = value
                .as_ref()
                .map(|val| serde_json::to_string(val).expect("serde_json::Value serialization should not fail"));
            let mirror_value_json = operation.mirror.as_ref().map(|mirror| {
                serde_json::to_string(&mirror.value).expect("mirror value serialization should not fail")
            });
            PatchOperationPayload {
                path: operation.path,
                op_type,
                value,
                value_json,
                mirror: operation.mirror,
                mirror_value_json,
            }
        })
        .collect();

    EntityPatch {
        key,
        entity_id,
        expected_version,
        operations: ops,
        idempotency_key,
        idempotency_ttl,
        relations: relation_mutations,
        unique_constraints,
    }
}

fn skip_false(value: &bool) -> bool {
    !*value
}
