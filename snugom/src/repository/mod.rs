use std::{borrow::Cow, marker::PhantomData};

const MAX_CASCADE_DEPTH: usize = 8;

use crate::{
    errors::{RepoError, ValidationError, ValidationIssue, ValidationResult},
    keys::KeyContext,
    registry,
    runtime::{
        MutationExecutor, RedisExecutor,
        commands::{
            CascadeDirective, CascadeRelationSpec, DeleteCascadeRelation, MutationCommand, MutationPlan,
            RelationMutation, UniqueConstraintCheck, UniqueConstraintDefinition, build_entity_delete,
            build_entity_mutation, build_entity_patch,
        },
    },
    search::{self, SearchEntity, SearchParams, SearchQuery, SearchResult},
    types::{
        BundleRegistered, CascadePolicy, DatetimeMirrorValue, EntityDescriptor, EntityMetadata, FieldDescriptor,
        FieldType, RelationKind, ValidationRule, ValidationScope,
    },
    validators::{is_valid_email, is_valid_url, is_valid_uuid},
};
use chrono::Utc;
use redis::{aio::ConnectionManager, cmd};
use regex::Regex;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Number, Value};

pub trait MutationPayloadBuilder {
    type Entity: EntityMetadata;

    fn into_payload(self) -> ValidationResult<MutationPayload>;
}

pub trait UpdatePatchBuilder {
    type Entity: EntityMetadata;

    fn into_patch(self) -> ValidationResult<MutationPatch>;
}

impl<T> Repo<T>
where
    T: BundleRegistered + DeserializeOwned,
{
    pub async fn get(&self, conn: &mut ConnectionManager, entity_id: &str) -> Result<Option<T>, RepoError> {
        let key = self.entity_key(entity_id);
        let result: Option<String> = cmd("JSON.GET").arg(&key).query_async(conn).await?;
        match result {
            Some(json) => {
                let value = serde_json::from_str::<T>(&json).map_err(|err| RepoError::Other {
                    message: format!("failed to deserialize entity: {err}").into(),
                })?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    pub async fn exists(&self, conn: &mut ConnectionManager, entity_id: &str) -> Result<bool, RepoError> {
        let key = self.entity_key(entity_id);
        let exists: i64 = cmd("EXISTS").arg(&key).query_async(conn).await?;
        Ok(exists == 1)
    }

    pub async fn count(&self, conn: &mut ConnectionManager) -> Result<u64, RepoError> {
        const SCAN_COUNT: usize = 1024;
        let pattern = format!(
            "{}:{}:{}:*",
            self.prefix, self.descriptor.service, self.descriptor.collection
        );
        let mut cursor: u64 = 0;
        let mut total: u64 = 0;
        loop {
            let (next_cursor, batch): (u64, Vec<String>) = cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(SCAN_COUNT)
                .query_async(conn)
                .await?;
            total += batch.len() as u64;
            cursor = next_cursor;
            if cursor == 0 {
                break;
            }
        }
        Ok(total)
    }
}

fn length_for_value(field_type: FieldType, value: &Value) -> Option<usize> {
    match field_type {
        FieldType::String | FieldType::DateTime => value.as_str().map(|s| s.chars().count()),
        FieldType::Array => value.as_array().map(|arr| arr.len()),
        _ => None,
    }
}

fn numeric_from_value(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(string) => string.parse::<f64>().ok(),
        _ => None,
    }
}

fn validate_rule_on_value(
    field_name: &str,
    field_type: FieldType,
    rule: &ValidationRule,
    value: &Value,
    issues: &mut Vec<ValidationIssue>,
) {
    match rule {
        ValidationRule::Length { min, max } => {
            if let Some(len) = length_for_value(field_type, value) {
                if let Some(min_len) = min
                    && len < *min_len {
                        issues.push(ValidationIssue::new(
                            field_name,
                            "validation.length",
                            format!("length must be at least {}", min_len),
                        ));
                    }
                if let Some(max_len) = max
                    && len > *max_len {
                        issues.push(ValidationIssue::new(
                            field_name,
                            "validation.length",
                            format!("length must be at most {}", max_len),
                        ));
                    }
            }
        }
        ValidationRule::Range { min, max } => {
            if let Some(candidate) = numeric_from_value(value) {
                if let Some(min_repr) = min
                    && let Ok(parsed_min) = min_repr.parse::<f64>()
                        && candidate < parsed_min {
                            issues.push(ValidationIssue::new(
                                field_name,
                                "validation.range",
                                format!("value must be at least {}", min_repr),
                            ));
                        }
                if let Some(max_repr) = max
                    && let Ok(parsed_max) = max_repr.parse::<f64>()
                        && candidate > parsed_max {
                            issues.push(ValidationIssue::new(
                                field_name,
                                "validation.range",
                                format!("value must be at most {}", max_repr),
                            ));
                        }
            }
        }
        ValidationRule::Regex { pattern } => {
            if let Some(candidate) = value.as_str()
                && Regex::new(pattern).map(|regex| !regex.is_match(candidate)).unwrap_or(false) {
                    issues.push(ValidationIssue::new(
                        field_name,
                        "validation.regex",
                        format!("value does not match pattern {}", pattern),
                    ));
                }
        }
        ValidationRule::Enum {
            allowed,
            case_insensitive,
        } => {
            if let Some(candidate) = value.as_str() {
                let candidate_norm = if *case_insensitive {
                    candidate.to_ascii_lowercase()
                } else {
                    candidate.to_string()
                };
                let mut normalized_allowed: Vec<String> = allowed.clone();
                if *case_insensitive {
                    normalized_allowed =
                        normalized_allowed.into_iter().map(|value| value.to_ascii_lowercase()).collect();
                }
                if !normalized_allowed.iter().any(|allowed| allowed == &candidate_norm) {
                    issues.push(ValidationIssue::new(
                        field_name,
                        "validation.enum",
                        format!("value must be one of {:?}", allowed),
                    ));
                }
            }
        }
        ValidationRule::Email => {
            if let Some(candidate) = value.as_str()
                && !is_valid_email(candidate) {
                    issues.push(ValidationIssue::new(
                        field_name,
                        "validation.email",
                        "value must be a valid email address",
                    ));
                }
        }
        ValidationRule::Url => {
            if let Some(candidate) = value.as_str()
                && !is_valid_url(candidate) {
                    issues.push(ValidationIssue::new(field_name, "validation.url", "value must be a valid URL"));
                }
        }
        ValidationRule::Uuid => {
            if let Some(candidate) = value.as_str()
                && !is_valid_uuid(candidate) {
                    issues.push(ValidationIssue::new(
                        field_name,
                        "validation.uuid",
                        "value must be a valid UUID",
                    ));
                }
        }
        ValidationRule::RequiredIf { .. }
        | ValidationRule::ForbiddenIf { .. }
        | ValidationRule::Unique { .. }
        | ValidationRule::Custom { .. } => {
            // These rules depend on wider entity context and are enforced during full entity validation.
            // Unique constraints are enforced at database level via Lua script.
        }
    }
}

fn validate_field_assignment(field: &FieldDescriptor, value: &Value) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    for descriptor in &field.validations {
        match descriptor.scope {
            ValidationScope::Field => {
                validate_rule_on_value(&field.name, field.field_type, &descriptor.rule, value, &mut issues);
            }
            ValidationScope::EachElement => {
                if let Some(array) = value.as_array() {
                    let element_type = field.element_type.unwrap_or(FieldType::Object);
                    for element in array {
                        validate_rule_on_value(&field.name, element_type, &descriptor.rule, element, &mut issues);
                    }
                }
            }
        }
    }
    issues
}

fn apply_patch_operations_to_value(target: &mut Value, operations: &[PatchOperation]) -> Result<(), RepoError> {
    for op in operations {
        let path = op.path.strip_prefix("$").unwrap_or(&op.path);
        let path = path.strip_prefix('.').unwrap_or(path);
        if path.is_empty() {
            continue;
        }
        let segments: Vec<&str> = path.split('.').filter(|segment| !segment.is_empty()).collect();
        if segments.is_empty() {
            continue;
        }
        match &op.kind {
            PatchOpKind::Assign(value) => set_value_at_path(target, &segments, value.clone())?,
            PatchOpKind::Merge(value) => merge_value_at_path(target, &segments, value.clone())?,
            PatchOpKind::Delete => delete_value_at_path(target, &segments)?,
        }
    }
    Ok(())
}

fn merge_value_at_path(target: &mut Value, segments: &[&str], patch: Value) -> Result<(), RepoError> {
    let key = segments.last().copied().unwrap_or("");
    let parent = parent_map_mut(target, &segments[..segments.len() - 1])?;
    match parent.get_mut(key) {
        Some(existing) => merge_json_values(existing, patch),
        None => {
            parent.insert(key.to_string(), patch);
        }
    }
    Ok(())
}

fn merge_json_values(target: &mut Value, patch: Value) {
    match (target, patch) {
        (Value::Object(target_map), Value::Object(patch_map)) => {
            for (key, value) in patch_map {
                match target_map.get_mut(&key) {
                    Some(existing) => merge_json_values(existing, value),
                    None => {
                        target_map.insert(key, value);
                    }
                }
            }
        }
        (target_slot, patch_value) => {
            *target_slot = patch_value;
        }
    }
}

fn set_value_at_path(target: &mut Value, segments: &[&str], value: Value) -> Result<(), RepoError> {
    if segments.is_empty() {
        return Err(RepoError::Validation(ValidationError::single(
            "",
            "patch.invalid_path",
            "path cannot be empty",
        )));
    }
    let key = segments.last().copied().unwrap_or("");
    let parent = parent_map_mut(target, &segments[..segments.len() - 1])?;
    parent.insert(key.to_string(), value);
    Ok(())
}

fn delete_value_at_path(target: &mut Value, segments: &[&str]) -> Result<(), RepoError> {
    if segments.is_empty() {
        return Err(RepoError::Validation(ValidationError::single(
            "",
            "patch.invalid_path",
            "path cannot be empty",
        )));
    }
    if segments.len() == 1 {
        if let Value::Object(map) = target {
            map.remove(segments[0]);
        }
        return Ok(());
    }
    let key = segments.last().copied().unwrap_or("");
    let parent = parent_map_mut(target, &segments[..segments.len() - 1])?;
    parent.remove(key);
    Ok(())
}

fn parent_map_mut<'a>(value: &'a mut Value, segments: &[&str]) -> Result<&'a mut Map<String, Value>, RepoError> {
    let mut current = value;
    for segment in segments {
        match current {
            Value::Object(map) => {
                current = map.entry((*segment).to_string()).or_insert_with(|| Value::Object(Map::new()));
            }
            _ => {
                return Err(RepoError::Validation(ValidationError::single(
                    (*segment).to_string(),
                    "patch.invalid_path",
                    "expected object while traversing patch path",
                )));
            }
        }
    }
    match current {
        Value::Object(map) => Ok(map),
        _ => Err(RepoError::Validation(ValidationError::single(
            segments.last().copied().unwrap_or("").to_string(),
            "patch.invalid_path",
            "expected object while applying patch",
        ))),
    }
}

fn validate_entity_json(descriptor: &EntityDescriptor, value: &Value) -> ValidationResult<()> {
    let object = value.as_object().ok_or_else(|| {
        ValidationError::single("__entity", "validation.invalid_type", "expected object for entity payload")
    })?;

    let mut issues = Vec::new();
    for field in &descriptor.fields {
        match object.get(&field.name) {
            Some(field_value) => {
                issues.extend(validate_field_assignment(field, field_value));
            }
            None => {
                // Skip required check for optional fields, auto-managed fields, and relation Vec fields
                if !field.optional && !field.auto_created && !field.auto_updated && !field.is_relation_vec {
                    issues.push(ValidationIssue::new(
                        field.name.clone(),
                        "validation.required",
                        "field is required",
                    ));
                }
            }
        }
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(ValidationError::new(issues))
    }
}

fn cascade_relation_specs_for(
    descriptor: &EntityDescriptor,
    stack: &mut Vec<(String, String)>,
    depth: usize,
) -> Result<Vec<CascadeRelationSpec>, RepoError> {
    if depth > MAX_CASCADE_DEPTH {
        return Err(RepoError::Other {
            message: Cow::Owned(format!(
                "cascade depth exceeded limit of {} at {}:{}",
                MAX_CASCADE_DEPTH, descriptor.service, descriptor.collection
            )),
        });
    }
    let mut specs = Vec::new();
    stack.push((descriptor.service.clone(), descriptor.collection.clone()));

    // 1. Process the entity's own declared relations (has_many, many_to_many)
    // Skip belongs_to relations - their cascade policy describes what happens when the PARENT is deleted,
    // not when THIS entity is deleted. Incoming belongs_to are handled in section 2.
    for relation in &descriptor.relations {
        if matches!(relation.kind, RelationKind::BelongsTo) {
            continue;
        }
        let directive = match relation.cascade {
            CascadePolicy::None => continue,
            CascadePolicy::Detach => CascadeDirective::DetachDependents,
            CascadePolicy::Delete => CascadeDirective::DeleteDependents,
        };

        let child_relations = if matches!(relation.cascade, CascadePolicy::Delete) {
            let service = relation.target_service.clone().unwrap_or_else(|| descriptor.service.clone());
            if stack.contains(&(service.clone(), relation.target.clone())) {
                return Err(RepoError::Other {
                    message: Cow::Owned(format!(
                        "cycle detected in cascade chain: {}:{}, relation {} -> {}:{}",
                        descriptor.service, descriptor.collection, relation.alias, service, relation.target
                    )),
                });
            }
            let target_descriptor =
                registry::get_descriptor(&service, &relation.target).ok_or_else(|| RepoError::Other {
                    message: Cow::Owned(format!(
                        "descriptor for service `{}` collection `{}` is not registered",
                        service, relation.target
                    )),
                })?;
            cascade_relation_specs_for(&target_descriptor, stack, depth + 1)?
        } else {
            Vec::new()
        };

        specs.push(CascadeRelationSpec {
            alias: relation.alias.clone(),
            target_collection: Some(relation.target.clone()),
            target_service: relation.target_service.clone(),
            cascade: directive,
            maintain_reverse: matches!(relation.kind, RelationKind::ManyToMany),
            child_relations,
        });
    }

    // 2. Process incoming belongs_to relations from other entities
    let incoming = registry::find_incoming_relations(&descriptor.service, &descriptor.collection);
    for inc in incoming {
        if !matches!(inc.kind, RelationKind::BelongsTo) {
            continue;
        }
        let directive = match inc.cascade {
            CascadePolicy::None => continue,
            CascadePolicy::Detach => CascadeDirective::DetachDependents,
            CascadePolicy::Delete => CascadeDirective::DeleteDependents,
        };

        // Check for cycles
        if stack.contains(&(inc.source_service.clone(), inc.source_collection.clone())) {
            return Err(RepoError::Other {
                message: Cow::Owned(format!(
                    "cycle detected in cascade chain via belongs_to: {}:{} -> {}:{}",
                    descriptor.service, descriptor.collection, inc.source_service, inc.source_collection
                )),
            });
        }

        let child_relations = if matches!(inc.cascade, CascadePolicy::Delete) {
            if let Some(child_desc) = registry::get_descriptor(&inc.source_service, &inc.source_collection) {
                cascade_relation_specs_for(&child_desc, stack, depth + 1)?
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // For incoming belongs_to, use reverse alias format
        let reverse_alias = format!("{}_reverse", inc.alias);
        specs.push(CascadeRelationSpec {
            alias: reverse_alias,
            target_collection: Some(inc.source_collection),
            target_service: Some(inc.source_service),
            cascade: directive,
            maintain_reverse: false,
            child_relations,
        });
    }

    stack.pop();
    Ok(specs)
}

fn delete_cascades_for_descriptor(
    descriptor: &EntityDescriptor,
    key_context: &KeyContext<'_>,
    entity_id: &str,
) -> Result<Vec<DeleteCascadeRelation>, RepoError> {
    let mut stack = Vec::new();

    // Get cascades from both the entity's own declared relations AND incoming belongs_to
    let specs = cascade_relation_specs_for(descriptor, &mut stack, 0)?;
    let mut cascades = Vec::new();
    for spec in specs {
        // Determine the relation key based on whether this is a reverse (incoming belongs_to) relation
        let relation_key = if spec.alias.ends_with("_reverse") {
            // For incoming belongs_to, the alias is "{original_alias}_reverse"
            // Extract the original alias and use relation_reverse key format
            let original_alias = spec.alias.strip_suffix("_reverse").unwrap_or(&spec.alias);
            key_context.relation_reverse(original_alias, entity_id)
        } else {
            // For the entity's own declared relations (has_many, many_to_many)
            key_context.relation(&spec.alias, entity_id)
        };

        cascades.push(DeleteCascadeRelation {
            alias: spec.alias,
            relation_key,
            target_collection: spec.target_collection.clone(),
            target_service: spec.target_service.clone(),
            cascade: spec.cascade,
            maintain_reverse: spec.maintain_reverse,
            child_relations: spec.child_relations,
        });
    }

    Ok(cascades)
}

#[derive(Debug, Clone)]
pub struct MutationPayload {
    pub entity_id: String,
    pub payload: Value,
    pub mirrors: Vec<DatetimeMirrorValue>,
    pub relations: Vec<RelationPlan>,
    pub nested: Vec<NestedMutation>,
    pub idempotency_key: Option<String>,
    pub idempotency_ttl: Option<u64>,
    pub managed_overrides: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum PatchOpKind {
    Assign(Value),
    Merge(Value),
    Delete,
}

#[derive(Debug, Clone)]
pub struct PatchOperation {
    pub path: String,
    pub kind: PatchOpKind,
    pub mirror: Option<DatetimeMirrorValue>,
}

#[derive(Debug, Clone)]
pub struct MutationPatch {
    pub entity_id: String,
    pub expected_version: Option<u64>,
    pub operations: Vec<PatchOperation>,
    pub relations: Vec<RelationPlan>,
    pub nested: Vec<NestedMutation>,
    pub idempotency_key: Option<String>,
    pub idempotency_ttl: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct CreateResult {
    pub id: String,
    pub responses: Vec<Value>,
}

#[derive(Debug, Clone)]
pub enum UpsertResult {
    Created(CreateResult),
    Updated(Vec<Value>),
}

#[derive(Debug, Clone)]
pub struct NestedMutation {
    pub alias: String,
    pub descriptor: EntityDescriptor,
    pub payload: MutationPayload,
}

pub fn link_nested_to_parent(parent_descriptor: &EntityDescriptor, parent_id: &str, nested: &mut [NestedMutation]) {
    if nested.is_empty() {
        return;
    }

    let parent_collection = parent_descriptor.collection.clone();
    let parent_service = parent_descriptor.service.clone();

    for mutation in nested.iter_mut() {
        let Some(parent_relation) = parent_descriptor
            .relations
            .iter()
            .find(|relation| relation.alias == mutation.alias)
        else {
            continue;
        };

        if parent_relation.target != mutation.descriptor.collection {
            continue;
        }

        let expected_service = parent_relation
            .target_service
            .as_ref()
            .cloned()
            .unwrap_or_else(|| parent_service.clone());
        if mutation.descriptor.service != expected_service {
            continue;
        }

        let child_relation = mutation.descriptor.relations.iter().find(|relation| {
            matches!(relation.kind, RelationKind::BelongsTo)
                && relation.target == parent_collection
                && relation
                    .target_service
                    .as_ref()
                    .map(|svc| svc == &parent_service)
                    .unwrap_or(true)
        });

        let Some(child_relation) = child_relation else {
            continue;
        };

        let parent_id_string = parent_id.to_string();

        if let Some(foreign_key) = parent_relation
            .foreign_key
            .as_ref()
            .or(child_relation.foreign_key.as_ref())
            && let Value::Object(map) = &mut mutation.payload.payload {
                map.insert(foreign_key.clone(), Value::String(parent_id_string.clone()));
            }

        let relation_alias = child_relation.alias.clone();
        let already_connected = mutation
            .payload
            .relations
            .iter()
            .any(|plan| plan.alias == relation_alias && plan.add.iter().any(|value| value == &parent_id_string));
        if !already_connected {
            mutation.payload.relations.push(RelationPlan::new(
                relation_alias,
                vec![parent_id_string.clone()],
                Vec::new(),
            ));
        }

        if let Some(derived_id) = apply_derived_id(&mutation.descriptor, &mut mutation.payload.payload) {
            mutation.payload.entity_id = derived_id;
        }
    }
}

#[derive(Debug, Clone)]
struct PendingRelationDelete {
    ids: Vec<String>,
    target_service: Option<String>,
    target_collection: String,
}

#[derive(Debug, Clone, Default)]
pub struct RelationPlan {
    pub alias: String,
    pub left_id: Option<String>,
    pub add: Vec<String>,
    pub remove: Vec<String>,
    pub delete: Vec<String>,
}

impl RelationPlan {
    pub fn new(alias: impl Into<String>, add: Vec<String>, remove: Vec<String>) -> Self {
        Self {
            alias: alias.into(),
            left_id: None,
            add,
            remove,
            delete: Vec::new(),
        }
    }

    pub fn with_left(
        alias: impl Into<String>,
        left_id: impl Into<String>,
        add: Vec<String>,
        remove: Vec<String>,
    ) -> Self {
        Self {
            alias: alias.into(),
            left_id: Some(left_id.into()),
            add,
            remove,
            delete: Vec::new(),
        }
    }
}

fn apply_derived_id(descriptor: &EntityDescriptor, payload: &mut Value) -> Option<String> {
    let derived = descriptor.derived_id.as_ref()?;
    let id_field = descriptor.id_field.as_ref()?;
    let object = payload.as_object_mut()?;

    let mut parts = Vec::with_capacity(derived.components.len());
    for field in &derived.components {
        let value = object.get(field)?.as_str()?.to_string();
        if value.is_empty() {
            return None;
        }
        parts.push(value);
    }

    let derived_id = parts.join(&derived.separator);
    object.insert(id_field.clone(), Value::String(derived_id.clone()));
    Some(derived_id)
}

impl<T> Repo<T>
where
    T: BundleRegistered + SearchEntity,
{
    /// Ensure the RediSearch index for this repository exists.
    pub async fn ensure_search_index(&self, conn: &mut ConnectionManager) -> Result<(), RepoError> {
        let definition = T::index_definition(&self.prefix);
        search::ensure_index(conn, &definition).await
    }

    /// Execute a search using pre-built parameters.
    pub async fn search(
        &self,
        conn: &mut ConnectionManager,
        params: SearchParams,
    ) -> Result<SearchResult<T>, RepoError> {
        let definition = T::index_definition(&self.prefix);
        let base_filter = T::base_filter();
        search::execute_search(conn, definition.name.as_str(), &params, &base_filter).await
    }

    /// Convenience helper mirroring the legacy manager's `with_text_query` flow.
    pub async fn search_with_query(
        &self,
        conn: &mut ConnectionManager,
        query: SearchQuery,
    ) -> Result<SearchResult<T>, RepoError> {
        let params = query.with_text_query(
            T::allowed_sorts(),
            T::default_sort(),
            |descriptor| T::map_filter(descriptor),
            T::text_search_fields(),
        )?;
        self.search(conn, params).await
    }
}

pub struct Repo<T>
where
    T: BundleRegistered,
{
    descriptor: EntityDescriptor,
    prefix: String,
    _marker: PhantomData<T>,
}

impl<T> Repo<T>
where
    T: BundleRegistered,
{
    pub fn new(prefix: impl Into<String>) -> Self {
        T::ensure_registered();
        Self {
            descriptor: T::entity_descriptor(),
            prefix: prefix.into(),
            _marker: PhantomData,
        }
    }

    pub fn descriptor(&self) -> &EntityDescriptor {
        &self.descriptor
    }

    pub fn key_context(&self) -> KeyContext<'_> {
        KeyContext::new(&self.prefix, &self.descriptor.service)
    }

    pub fn entity_key(&self, entity_id: &str) -> String {
        self.key_context().entity(&self.descriptor.collection, entity_id)
    }

    pub fn relation_key(&self, alias: &str, left_id: &str) -> String {
        self.key_context().relation(alias, left_id)
    }

    pub fn relation_reverse_key(&self, alias: &str, right_id: &str) -> String {
        self.key_context().relation_reverse(alias, right_id)
    }

    pub async fn execute<E>(&self, executor: &mut E, plan: MutationPlan) -> Result<Vec<Value>, RepoError>
    where
        E: MutationExecutor + ?Sized,
    {
        executor.execute(plan).await
    }

    pub async fn create<E, B>(&self, executor: &mut E, builder: B) -> Result<CreateResult, RepoError>
    where
        E: MutationExecutor + ?Sized,
        B: MutationPayloadBuilder,
        B::Entity: EntityMetadata,
    {
        let MutationPayload {
            mut entity_id,
            mut payload,
            mirrors,
            relations,
            nested,
            idempotency_key,
            idempotency_ttl,
            managed_overrides,
        } = builder.into_payload()?;
        let overrides: ::std::collections::BTreeSet<_> = managed_overrides.into_iter().collect();
        let mut mirrors = mirrors;
        ensure_auto_timestamps(self.descriptor(), &mut payload, &mut mirrors, &overrides, false);
        ensure_metadata_object(&mut payload);
        inject_enum_tag_shadows(self.descriptor(), &mut payload);
        if let Some(derived_id) = apply_derived_id(self.descriptor(), &mut payload) {
            entity_id = derived_id;
        }
        if let Err(err) = validate_entity_json(self.descriptor(), &payload) {
            return Err(RepoError::Validation(err));
        }
        let mut nested = nested;
        link_nested_to_parent(self.descriptor(), &entity_id, &mut nested);
        self.execute_nested(executor, nested).await?;
        let key = self.entity_key(&entity_id);
        let key_context = self.key_context();
        let (relation_mutations, pending_deletes) =
            Self::relation_mutations_for(self.descriptor(), &key_context, Some(&entity_id), relations)?;
        let mut plan = MutationPlan::new();
        let mutation = build_entity_mutation(
            self.descriptor(),
            key,
            payload,
            mirrors,
            None,
            idempotency_key,
            idempotency_ttl,
            relation_mutations,
        )?;
        plan.push(MutationCommand::UpsertEntity(mutation));
        Self::enqueue_relation_deletes_for_context(&key_context, self.descriptor(), pending_deletes, &mut plan)?;
        let responses = self.execute(executor, plan).await?;
        if let Some(actual_id) = responses
            .last()
            .and_then(|value| value.get("entity_id"))
            .and_then(|value| value.as_str())
        {
            entity_id = actual_id.to_string();
        }
        Ok(CreateResult {
            id: entity_id,
            responses,
        })
    }

    pub async fn delete<E>(
        &self,
        executor: &mut E,
        entity_id: &str,
        expected_version: Option<u64>,
    ) -> Result<Vec<Value>, RepoError>
    where
        E: MutationExecutor + ?Sized,
    {
        let key_context = self.key_context();
        let key = key_context.entity(&self.descriptor.collection, entity_id);
        let cascades = delete_cascades_for_descriptor(self.descriptor(), &key_context, entity_id)?;
        let unique_constraints = unique_constraint_definitions_for(self.descriptor());
        let delete = build_entity_delete(key, expected_version, cascades, unique_constraints);
        let mut plan = MutationPlan::new();
        plan.push(MutationCommand::DeleteEntity(delete));
        self.execute(executor, plan).await
    }

    pub async fn update_patch<E, B>(&self, executor: &mut E, builder: B) -> Result<Vec<Value>, RepoError>
    where
        E: MutationExecutor + ?Sized,
        B: UpdatePatchBuilder,
        B::Entity: EntityMetadata,
    {
        let patch = builder.into_patch()?;
        self.execute_patch(executor, patch).await
    }

    async fn execute_patch<E>(&self, executor: &mut E, patch: MutationPatch) -> Result<Vec<Value>, RepoError>
    where
        E: MutationExecutor + ?Sized,
        T: EntityMetadata,
    {
        let MutationPatch {
            entity_id,
            expected_version,
            mut operations,
            relations,
            mut nested,
            idempotency_key,
            idempotency_ttl,
        } = patch;

        if operations.is_empty() && relations.is_empty() && nested.is_empty() {
            return Ok(Vec::new());
        }

        if !nested.is_empty() {
            link_nested_to_parent(self.descriptor(), &entity_id, &mut nested);
            self.execute_nested(executor, ::std::mem::take(&mut nested)).await?;
        }

        let key_context = self.key_context();
        let key = key_context.entity(&self.descriptor.collection, &entity_id);
        let (relation_mutations, pending_deletes) =
            Self::relation_mutations_for(self.descriptor(), &key_context, Some(&entity_id), relations)?;
        let mut validation_issues = Vec::new();

        for op in &operations {
            let field_name = op.path.strip_prefix("$.").unwrap_or(op.path.as_str());
            let descriptor_field = self
                .descriptor
                .fields
                .iter()
                .find(|field| field.name == field_name)
                .ok_or_else(|| {
                    RepoError::Validation(ValidationError::single(
                        field_name,
                        "patch.unknown_field",
                        format!("field `{}` is not defined on entity", field_name),
                    ))
                })?;

            if descriptor_field.is_id {
                return Err(RepoError::Validation(ValidationError::single(
                    field_name,
                    "patch.immutable_field",
                    "cannot patch identifier field",
                )));
            }

            if matches!(op.kind, PatchOpKind::Delete) && !descriptor_field.optional {
                return Err(RepoError::Validation(ValidationError::single(
                    field_name,
                    "patch.non_optional_delete",
                    "field cannot be deleted because it is not optional",
                )));
            }

            if let PatchOpKind::Assign(value) = &op.kind {
                validation_issues.extend(validate_field_assignment(descriptor_field, value));
            }
        }

        if !validation_issues.is_empty() {
            return Err(RepoError::Validation(ValidationError::new(validation_issues)));
        }

        // Build unique constraint checks for fields being patched
        let unique_constraints = build_patch_unique_constraint_checks(self.descriptor(), &operations);

        for field in &self.descriptor.fields {
            if !field.auto_updated {
                continue;
            }

            let path = format!("$.{}", field.name);
            if operations.iter().any(|op| op.path == path) {
                continue;
            }

            let now = Utc::now();
            let iso = now.to_rfc3339();
            let mirror_value = now.timestamp_millis();
            let mirror = field.datetime_mirror.as_ref().map(|mirror_field| {
                DatetimeMirrorValue::new(field.name.clone(), mirror_field.clone(), Some(mirror_value))
            });

            operations.push(PatchOperation {
                path,
                kind: PatchOpKind::Assign(Value::String(iso)),
                mirror,
            });
        }

        // Inject shadow tag operations for any enum fields being patched
        inject_enum_tag_shadow_operations(self.descriptor(), &mut operations);

        let patch_command = build_entity_patch(
            key,
            Some(entity_id.clone()),
            expected_version,
            operations,
            idempotency_key,
            idempotency_ttl,
            relation_mutations,
            unique_constraints,
        );

        let mut plan = MutationPlan::new();
        plan.push(MutationCommand::PatchEntity(patch_command));
        Self::enqueue_relation_deletes_for_context(&key_context, self.descriptor(), pending_deletes, &mut plan)?;
        self.execute(executor, plan).await
    }

    pub async fn mutate_relations<E>(
        &self,
        executor: &mut E,
        relations: Vec<RelationPlan>,
    ) -> Result<Vec<Value>, RepoError>
    where
        E: MutationExecutor + ?Sized,
    {
        if relations.is_empty() {
            return Ok(Vec::new());
        }
        let key_context = self.key_context();
        let (relation_mutations, pending_deletes) =
            Self::relation_mutations_for(self.descriptor(), &key_context, None, relations).map_err(RepoError::from)?;
        if relation_mutations.is_empty() && pending_deletes.is_empty() {
            return Ok(Vec::new());
        }
        let mut plan = MutationPlan::new();
        for relation in relation_mutations {
            plan.push(MutationCommand::MutateRelations(relation));
        }
        Self::enqueue_relation_deletes_for_context(&key_context, self.descriptor(), pending_deletes, &mut plan)?;
        self.execute(executor, plan).await
    }

    pub async fn create_with_conn<B>(&self, conn: &mut ConnectionManager, builder: B) -> Result<CreateResult, RepoError>
    where
        B: MutationPayloadBuilder,
        B::Entity: EntityMetadata,
    {
        let mut executor = RedisExecutor::new(conn);
        self.create(&mut executor, builder).await
    }

    pub async fn update_patch_with_conn<B>(
        &self,
        conn: &mut ConnectionManager,
        builder: B,
    ) -> Result<Vec<Value>, RepoError>
    where
        B: UpdatePatchBuilder,
        B::Entity: EntityMetadata,
        T: EntityMetadata + Serialize + DeserializeOwned,
    {
        let patch = builder.into_patch()?;
        self.validate_patch_against_entity(conn, &patch).await?;
        let mut executor = RedisExecutor::new(conn);
        self.execute_patch(&mut executor, patch).await
    }

    pub async fn delete_with_conn(
        &self,
        conn: &mut ConnectionManager,
        entity_id: &str,
        expected_version: Option<u64>,
    ) -> Result<Vec<Value>, RepoError> {
        let mut executor = RedisExecutor::new(conn);
        self.delete(&mut executor, entity_id, expected_version).await
    }

    pub async fn mutate_relations_with_conn(
        &self,
        conn: &mut ConnectionManager,
        relations: Vec<RelationPlan>,
    ) -> Result<Vec<Value>, RepoError> {
        let mut executor = RedisExecutor::new(conn);
        self.mutate_relations(&mut executor, relations).await
    }

    async fn validate_patch_against_entity(
        &self,
        conn: &mut ConnectionManager,
        patch: &MutationPatch,
    ) -> Result<(), RepoError>
    where
        T: EntityMetadata + Serialize + DeserializeOwned,
    {
        if patch.operations.is_empty() {
            return Ok(());
        }

        let current = self.get(conn, &patch.entity_id).await?.ok_or_else(|| RepoError::NotFound {
            entity_id: Some(patch.entity_id.clone()),
        })?;

        let mut json = serde_json::to_value(&current).map_err(|err| {
            RepoError::Validation(ValidationError::single("__patch", "serialization.failed", err.to_string()))
        })?;

        apply_patch_operations_to_value(&mut json, &patch.operations)?;

        if let Err(err) = validate_entity_json(self.descriptor(), &json) {
            return Err(RepoError::Validation(err));
        }

        serde_json::from_value::<T>(json).map_err(|err| {
            RepoError::Validation(ValidationError::single("__patch", "deserialization.failed", err.to_string()))
        })?;
        Ok(())
    }
}

impl<T> Repo<T>
where
    T: BundleRegistered,
{
    fn relation_mutations_for(
        descriptor: &EntityDescriptor,
        key_context: &KeyContext<'_>,
        default_left: Option<&str>,
        plans: Vec<RelationPlan>,
    ) -> ValidationResult<(Vec<RelationMutation>, Vec<PendingRelationDelete>)> {
        let mut issues = Vec::new();
        let mut mutations = Vec::new();
        let mut deletes = Vec::new();

        for plan in plans {
            let RelationPlan {
                alias,
                left_id,
                add,
                mut remove,
                delete,
            } = plan;

            let relation_info = descriptor.relations.iter().find(|relation| relation.alias == alias);
            let relation_descriptor = match relation_info {
                Some(info) => info,
                None => {
                    issues.push(ValidationIssue::new(
                        format!("relations.{}", alias),
                        "relation.unknown_alias",
                        "relation alias is not defined on this entity",
                    ));
                    continue;
                }
            };

            // Maintain reverse index for:
            // - ManyToMany (bidirectional by nature)
            // - BelongsTo with cascade (so parent can find children during delete)
            let maintain_reverse = matches!(relation_descriptor.kind, RelationKind::ManyToMany)
                || (matches!(relation_descriptor.kind, RelationKind::BelongsTo)
                    && !matches!(relation_descriptor.cascade, CascadePolicy::None));
            let left_value = left_id.or_else(|| default_left.map(|value| value.to_string()));

            match left_value {
                Some(left) => {
                    let relation_key = key_context.relation(&alias, &left);

                    for value in &delete {
                        remove.push(value.clone());
                    }

                    let cascade = if !delete.is_empty() && matches!(relation_descriptor.cascade, CascadePolicy::Delete)
                    {
                        deletes.push(PendingRelationDelete {
                            ids: delete,
                            target_service: relation_descriptor.target_service.clone(),
                            target_collection: relation_descriptor.target.clone(),
                        });
                        Some(CascadeDirective::DeleteDependents)
                    } else {
                        None
                    };

                    mutations.push(RelationMutation {
                        relation_key,
                        add,
                        remove,
                        cascade,
                        maintain_reverse,
                    });
                }
                None => {
                    issues.push(ValidationIssue::new(
                        format!("relations.{}", alias),
                        "relation.left_id_missing",
                        "left id must be provided",
                    ));
                }
            }
        }
        if issues.is_empty() {
            Ok((mutations, deletes))
        } else {
            Err(ValidationError::new(issues))
        }
    }

    async fn execute_nested<E>(&self, executor: &mut E, nested: Vec<NestedMutation>) -> Result<(), RepoError>
    where
        E: MutationExecutor + ?Sized,
    {
        enum NestedTask {
            Process(NestedMutation),
            Execute(NestedMutation),
        }

        let mut stack: Vec<NestedTask> = nested.into_iter().map(NestedTask::Process).collect();

        while let Some(task) = stack.pop() {
            match task {
                NestedTask::Process(mut mutation) => {
                    let children = ::std::mem::take(&mut mutation.payload.nested);
                    stack.push(NestedTask::Execute(mutation));
                    for child in children.into_iter() {
                        stack.push(NestedTask::Process(child));
                    }
                }
                NestedTask::Execute(mut mutation) => {
                    let key_context = KeyContext::new(&self.prefix, &mutation.descriptor.service);
                    let key = key_context.entity(&mutation.descriptor.collection, &mutation.payload.entity_id);
                    let mirrors = ::std::mem::take(&mut mutation.payload.mirrors);
                    let relations = ::std::mem::take(&mut mutation.payload.relations);
                    let idempotency_key = mutation.payload.idempotency_key.take();
                    let idempotency_ttl = mutation.payload.idempotency_ttl.take();
                    let (relation_mutations, pending_deletes) = Self::relation_mutations_for(
                        &mutation.descriptor,
                        &key_context,
                        Some(&mutation.payload.entity_id),
                        relations,
                    )?;
                    ensure_metadata_object(&mut mutation.payload.payload);
                    inject_enum_tag_shadows(&mutation.descriptor, &mut mutation.payload.payload);
                    if let Err(err) = validate_entity_json(&mutation.descriptor, &mutation.payload.payload) {
                        return Err(RepoError::Validation(err));
                    }
                    let mutation_command = build_entity_mutation(
                        &mutation.descriptor,
                        key,
                        mutation.payload.payload,
                        mirrors,
                        None,
                        idempotency_key,
                        idempotency_ttl,
                        relation_mutations,
                    )?;
                    let mut plan = MutationPlan::new();
                    plan.push(MutationCommand::UpsertEntity(mutation_command));
                    Self::enqueue_relation_deletes_for_context(
                        &key_context,
                        &mutation.descriptor,
                        pending_deletes,
                        &mut plan,
                    )?;
                    executor.execute(plan).await?;
                }
            }
        }

        Ok(())
    }

    fn enqueue_relation_deletes_for_context(
        key_context: &KeyContext<'_>,
        descriptor: &EntityDescriptor,
        deletes: Vec<PendingRelationDelete>,
        plan: &mut MutationPlan,
    ) -> Result<(), RepoError> {
        if deletes.is_empty() {
            return Ok(());
        }

        for pending in deletes {
            let target_service = pending.target_service.unwrap_or_else(|| descriptor.service.clone());
            let target_descriptor =
                registry::get_descriptor(&target_service, &pending.target_collection).ok_or_else(|| {
                    RepoError::Other {
                        message: Cow::Owned(format!(
                            "descriptor for service `{}` collection `{}` is not registered",
                            target_service, pending.target_collection
                        )),
                    }
                })?;

            let child_context = KeyContext::new(key_context.prefix, target_service.as_str());

            for id in pending.ids {
                let cascades = delete_cascades_for_descriptor(&target_descriptor, &child_context, &id)?;
                let unique_constraints = unique_constraint_definitions_for(&target_descriptor);
                let child_key = child_context.entity(&target_descriptor.collection, &id);
                let delete = build_entity_delete(child_key, None, cascades, unique_constraints);
                plan.push(MutationCommand::DeleteEntity(delete));
            }
        }

        Ok(())
    }
}

/// Extracts unique constraint definitions from an entity descriptor for delete cleanup.
fn unique_constraint_definitions_for(descriptor: &EntityDescriptor) -> Vec<UniqueConstraintDefinition> {
    descriptor
        .unique_constraints
        .iter()
        .map(|constraint| UniqueConstraintDefinition {
            fields: constraint.fields.clone(),
            case_insensitive: constraint.case_insensitive,
        })
        .collect()
}

/// Builds unique constraint checks for patch operations.
/// Only includes constraints where at least one field is being updated.
fn build_patch_unique_constraint_checks(
    descriptor: &EntityDescriptor,
    operations: &[PatchOperation],
) -> Vec<UniqueConstraintCheck> {
    // Collect the field names being patched
    let patched_fields: std::collections::HashSet<&str> = operations
        .iter()
        .filter_map(|op| {
            let field_name = op.path.strip_prefix("$.")?;
            // Only consider Assign operations (not Delete or Merge without value)
            if matches!(op.kind, PatchOpKind::Assign(_)) {
                Some(field_name)
            } else {
                None
            }
        })
        .collect();

    if patched_fields.is_empty() {
        return Vec::new();
    }

    // For each unique constraint, check if any of its fields are being patched
    descriptor
        .unique_constraints
        .iter()
        .filter_map(|constraint| {
            // Check if at least one field in the constraint is being patched
            let fields_being_patched: Vec<&str> = constraint
                .fields
                .iter()
                .filter(|f| patched_fields.contains(f.as_str()))
                .map(|f| f.as_str())
                .collect();

            if fields_being_patched.is_empty() {
                return None;
            }

            // Extract the new values from the patch operations
            let mut values = Vec::with_capacity(constraint.fields.len());
            for field_name in &constraint.fields {
                // Find the patch operation for this field
                let value = operations
                    .iter()
                    .find_map(|op| {
                        let op_field = op.path.strip_prefix("$.")?;
                        if op_field == field_name {
                            if let PatchOpKind::Assign(v) = &op.kind {
                                Some(v.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .unwrap_or(Value::Null); // Null means "read from entity"
                values.push(value);
            }

            Some(UniqueConstraintCheck {
                fields: constraint.fields.clone(),
                case_insensitive: constraint.case_insensitive,
                values,
            })
        })
        .collect()
}

fn ensure_auto_timestamps(
    descriptor: &EntityDescriptor,
    payload: &mut Value,
    mirrors: &mut Vec<DatetimeMirrorValue>,
    overrides: &::std::collections::BTreeSet<String>,
    force: bool,
) {
    let Some(object) = payload.as_object_mut() else {
        return;
    };

    for field in &descriptor.fields {
        let manages_created = field.auto_created;
        let manages_updated = field.auto_updated;
        if !manages_created && !manages_updated {
            continue;
        }

        let field_name = &field.name;
        if overrides.contains(field_name) {
            continue;
        }

        let already_present = object.contains_key(field_name);
        let should_update = if manages_updated {
            force || !already_present
        } else {
            !already_present
        };

        if !should_update {
            continue;
        }

        let now = Utc::now();
        let iso = now.to_rfc3339();
        let millis = now.timestamp_millis();

        object.insert(field_name.clone(), Value::String(iso));

        mirrors.retain(|entry| entry.field != *field_name);

        if let Some(mirror_field) = &field.datetime_mirror {
            object.insert(mirror_field.clone(), Value::Number(Number::from(millis)));
            mirrors.push(DatetimeMirrorValue::new(field_name.clone(), mirror_field.clone(), Some(millis)));
        }
    }
}

/// Ensures the payload has a `metadata` object so Lua scripts can set version fields.
fn ensure_metadata_object(payload: &mut Value) {
    if let Some(object) = payload.as_object_mut() {
        object
            .entry("metadata".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
    }
}

/// Injects shadow tag fields for enum fields that need them for RediSearch indexing.
///
/// For enum fields marked with `#[snugom(filterable)]` and `normalize_enum_tag: true`,
/// enums with associated data serialize to JSON objects (e.g., `{"swiss": {"rounds": 6}}`),
/// which RediSearch cannot index as TAG fields. This function adds a shadow field
/// (e.g., `__format_tag: "swiss"`) containing just the variant name that RediSearch can index.
///
/// The original field value is preserved for proper deserialization.
/// Unit variant enums that already serialize to strings don't need shadow fields,
/// but we add them anyway for consistency (the value will match the original).
fn inject_enum_tag_shadows(descriptor: &EntityDescriptor, payload: &mut Value) {
    let Some(object) = payload.as_object_mut() else {
        return;
    };

    for field in &descriptor.fields {
        if !field.normalize_enum_tag {
            continue;
        }

        let Some(field_value) = object.get(&field.name) else {
            continue;
        };

        // Extract the discriminant from the field value
        let discriminant = match field_value {
            Value::String(s) => Some(s.clone()),
            Value::Object(map) => map.keys().next().cloned(),
            _ => None,
        };

        if let Some(tag) = discriminant {
            let shadow_name = format!("__{}_tag", field.name);
            object.insert(shadow_name, Value::String(tag));
        }
    }
}

/// Injects shadow tag operations for enum fields in patch operations.
///
/// When a field with `normalize_enum_tag: true` is being patched, this function
/// adds a corresponding operation for the shadow field containing the discriminant.
fn inject_enum_tag_shadow_operations(descriptor: &EntityDescriptor, operations: &mut Vec<PatchOperation>) {
    let mut shadow_ops: Vec<PatchOperation> = Vec::new();

    for op in operations.iter() {
        let field_name = op.path.strip_prefix("$.").unwrap_or(op.path.as_str());

        // Find the field descriptor
        let Some(field) = descriptor.fields.iter().find(|f| f.name == field_name) else {
            continue;
        };

        if !field.normalize_enum_tag {
            continue;
        }

        let shadow_path = format!("$.__{}_tag", field.name);

        match &op.kind {
            PatchOpKind::Assign(value) => {
                // Extract discriminant from the value
                let discriminant = match value {
                    Value::String(s) => Some(s.clone()),
                    Value::Object(map) => map.keys().next().cloned(),
                    _ => None,
                };
                if let Some(tag) = discriminant {
                    shadow_ops.push(PatchOperation {
                        path: shadow_path,
                        kind: PatchOpKind::Assign(Value::String(tag)),
                        mirror: None,
                    });
                }
            }
            PatchOpKind::Delete => {
                // If the field is deleted, also delete the shadow
                shadow_ops.push(PatchOperation {
                    path: shadow_path,
                    kind: PatchOpKind::Delete,
                    mirror: None,
                });
            }
            PatchOpKind::Merge(_) => {
                // Merge operations don't change the discriminant, so no shadow update needed
            }
        }
    }

    operations.extend(shadow_ops);
}
