//! CollectionHandle - Type-safe accessor for CRUD operations on a single entity collection.
//!
//! This provides the Prisma-style API for simple operations:
//! ```ignore
//! let guild = snugom.guilds().get(&id).await?;
//! let guild = snugom.guilds().create(guild_data).await?;
//! let guilds = snugom.guilds().find_many(query).await?;
//! ```

use redis::aio::ConnectionManager;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{
    errors::RepoError,
    registry,
    repository::{
        CreateResult, FindChildResult, FindIncludeSpec, FindResult, GetOrCreateResult, MutationPayloadBuilder,
        RelationMetadata, Repo, UpdatePatchBuilder, UpsertResult,
    },
    search::{self, SearchQuery, SearchResult},
    types::{EntityMetadata, RelationData, RelationKind, RelationQueryOptions, SnugomModel},
};

/// An include resolved to "search a child collection by a foreign key equal to this
/// entity's id". Both forward (has_many with options) and reverse (incoming belongs_to)
/// includes reduce to this shape.
struct SearchInclude {
    /// Key under which results are stored in `FindResult.includes`.
    response_key: String,
    child_collection: String,
    foreign_key: String,
    opts: RelationQueryOptions,
}

/// Incoming `belongs_to` relations from `child_collection` to `parent_collection` that carry
/// a foreign key, optionally filtered to a specific relation alias. This is the single
/// registry-driven discovery routine used for both forward-with-options and reverse includes.
fn incoming_belongs_to(
    parent_collection: &str,
    child_collection: &str,
    wanted_alias: Option<&str>,
) -> Vec<registry::IncomingRelation> {
    registry::find_incoming_relations(parent_collection)
        .into_iter()
        .filter(|inc| {
            matches!(inc.kind, RelationKind::BelongsTo)
                && inc.source_collection == child_collection
                && inc.foreign_key.is_some()
                && wanted_alias.is_none_or(|a| inc.alias == a)
        })
        .collect()
}

/// The foreign key on `child_collection` pointing back at `parent_collection` (first match,
/// optionally filtered by alias). Resolves has_many-with-options to a searchable FK.
fn child_foreign_key_to(
    child_collection: &str,
    parent_collection: &str,
    wanted_alias: Option<&str>,
) -> Option<String> {
    incoming_belongs_to(parent_collection, child_collection, wanted_alias)
        .into_iter()
        .next()
        .and_then(|inc| inc.foreign_key)
}

/// Parse an include `filter` option (`field:op:value`) into a `FilterCondition`.
/// Supports `eq`, `bool`, and `range` (`min..max`, open-ended bounds allowed).
/// Range uses `..` (not a comma) so it survives the comma-split `?include=` query parser.
fn parse_include_filter(filter: &str) -> Result<search::FilterCondition, RepoError> {
    let invalid = |msg: String| RepoError::InvalidRequest { message: msg };
    let parts: Vec<&str> = filter.splitn(3, ':').collect();
    if parts.len() != 3 {
        return Err(invalid(format!("invalid include filter '{filter}', expected field:op:value")));
    }
    let (field, value) = (parts[0].trim(), parts[2].trim());
    match parts[1].to_ascii_lowercase().as_str() {
        "eq" => Ok(search::FilterCondition::tag_eq(field, value)),
        "bool" | "boolean" => value
            .parse::<bool>()
            .map(|b| search::FilterCondition::bool_eq(field, b))
            .map_err(|_| invalid(format!("invalid bool in include filter '{filter}'"))),
        "range" => {
            let bound = |s: &str| -> Result<Option<f64>, RepoError> {
                let s = s.trim();
                if s.is_empty() {
                    Ok(None)
                } else {
                    s.parse::<f64>().map(Some).map_err(|_| invalid(format!("invalid range bound in '{filter}'")))
                }
            };
            let (min, max) = value.split_once("..").ok_or_else(|| invalid(format!("range filter '{filter}' needs min..max")))?;
            Ok(search::FilterCondition::NumericRange {
                field: field.to_string(),
                min: bound(min)?,
                max: bound(max)?,
            })
        }
        other => Err(invalid(format!("unsupported include filter operator '{other}' (supported: eq, bool, range)"))),
    }
}

/// The field a leaf filter condition targets, for validation against the child schema.
fn filter_condition_field(condition: &search::FilterCondition) -> Option<&str> {
    match condition {
        search::FilterCondition::TagEquals { field, .. }
        | search::FilterCondition::NumericRange { field, .. }
        | search::FilterCondition::BooleanEquals { field, .. } => Some(field.as_str()),
        _ => None,
    }
}

/// Reject a filter/sort field that the child entity doesn't declare. The descriptor's
/// fields are a superset of the indexed ones, so this never rejects a valid query; it just
/// turns a typo'd field into a clear 400 instead of empty results (filter) or a 500 (sort).
/// When the child isn't registered there's nothing to check against, so it passes through.
fn ensure_known_field(
    descriptor: Option<&crate::types::EntityDescriptor>,
    collection: &str,
    field: &str,
) -> Result<(), RepoError> {
    match descriptor {
        Some(desc) if !desc.fields.iter().any(|f| f.name == field) => Err(RepoError::InvalidRequest {
            message: format!("unknown include field '{field}' on '{collection}'"),
        }),
        _ => Ok(()),
    }
}

/// Result of a bulk create operation.
#[derive(Debug, Clone)]
pub struct BulkCreateResult {
    /// Number of entities created
    pub count: u64,
    /// IDs of created entities
    pub ids: Vec<String>,
    /// Raw responses from Redis
    pub responses: Vec<Vec<Value>>,
}

/// Type-safe handle for CRUD operations on a single entity collection.
///
/// This struct provides the Prisma-style API for simple CRUD operations.
/// It owns a `Repo<T>` and a cloned `ConnectionManager`, allowing for
/// convenient method chaining without lifetime concerns.
///
/// # Example
/// ```ignore
/// // Get by ID
/// let guild = snugom.guilds().get(&id).await?;
///
/// // Create
/// let guild = snugom.guilds().create(guild_builder).await?;
///
/// // Query
/// let guilds = snugom.guilds().find_many(query).await?;
/// ```
pub struct CollectionHandle<T>
where
    T: SnugomModel,
{
    repo: Repo<T>,
    conn: ConnectionManager,
}

impl<T> CollectionHandle<T>
where
    T: SnugomModel,
{
    /// Create a new CollectionHandle with the given repo and connection.
    ///
    /// This is typically called via `Client::collection<T>()` or via
    /// named accessors generated by `#[derive(SnugomClient)]`.
    pub fn new(
        repo: Repo<T>,
        conn: ConnectionManager,
    ) -> Self {
        Self {
            repo,
            conn,
        }
    }

    /// Get a mutable reference to the connection for advanced operations.
    pub fn connection_mut(&mut self) -> &mut ConnectionManager {
        &mut self.conn
    }

    /// Get a reference to the underlying repository.
    pub fn repo(&self) -> &Repo<T> {
        &self.repo
    }

    /// Get the Redis key for an entity by ID.
    ///
    /// This provides access to the key format derived from the entity's
    /// `#[snugom(collection)]` attribute without hardcoding.
    ///
    /// # Example
    /// ```ignore
    /// let key = snugom.auctions().entity_key("auction-123");
    /// conn.expire(&key, 3600).await?;
    /// ```
    pub fn entity_key(
        &self,
        id: &str,
    ) -> String {
        self.repo.entity_key(id)
    }

    /// Get a glob pattern matching all entities in this collection.
    ///
    /// Useful for test cleanup or batch operations like `KEYS` or `SCAN`.
    ///
    /// # Example
    /// ```ignore
    /// let pattern = snugom.guilds().collection_pattern();
    /// // Returns something like "snug:guilds:*"
    /// let keys: Vec<String> = conn.keys(&pattern).await?;
    /// ```
    pub fn collection_pattern(&self) -> String {
        self.repo.collection_pattern()
    }
}

// ============ Single Record by ID ============

impl<T> CollectionHandle<T>
where
    T: SnugomModel + DeserializeOwned,
{
    /// Get entity by ID.
    ///
    /// Returns `None` if the entity doesn't exist.
    pub async fn get(
        &mut self,
        id: &str,
    ) -> Result<Option<T>, RepoError> {
        self.repo.get(&mut self.conn, id).await
    }

    /// Get entity by ID, returning an error if not found.
    ///
    /// This is equivalent to Prisma's `findUniqueOrThrow`.
    pub async fn get_or_error(
        &mut self,
        id: &str,
    ) -> Result<T, RepoError> {
        self.get(id).await?.ok_or(RepoError::NotFound {
            entity_id: Some(id.to_string()),
        })
    }

    /// Check if an entity exists by ID.
    pub async fn exists(
        &mut self,
        id: &str,
    ) -> Result<bool, RepoError> {
        self.repo.exists(&mut self.conn, id).await
    }

    /// Count all entities in the collection.
    pub async fn count(&mut self) -> Result<u64, RepoError> {
        self.repo.count(&mut self.conn).await
    }

    /// Batch-fetch entities by IDs using JSON.MGET.
    ///
    /// Returns only the entities that exist (missing IDs are silently skipped).
    pub async fn get_many(
        &mut self,
        ids: &[&str],
    ) -> Result<Vec<T>, RepoError> {
        self.repo.get_many(&mut self.conn, ids).await
    }

    /// Atomic find: get entity + related children in a single Lua script call.
    ///
    /// Uses `entity_find.lua` to atomically read the parent entity and all
    /// included relations (with recursive nesting) in one Redis call.
    pub async fn find_with_includes(
        &mut self,
        entity_id: &str,
        include_specs: Vec<FindIncludeSpec>,
    ) -> Result<FindResult, RepoError> {
        self.repo
            .find_with_includes(&mut self.conn, entity_id, include_specs)
            .await
    }

    /// Resolve `?include=` params into the parent plus its included children.
    ///
    /// Forward has_many includes are keyed by the relation alias; reverse (incoming
    /// belongs_to) includes by the child collection name, or `collection.alias` to
    /// disambiguate when a child has multiple foreign keys to this parent. Searched
    /// includes (forward-with-options and all reverse) return flat — they do not
    /// resolve their own nested includes.
    pub async fn find_with_includes_from_params(
        &mut self,
        entity_id: &str,
        params: &std::collections::HashMap<String, RelationQueryOptions>,
    ) -> Result<FindResult, RepoError> {
        let descriptor = T::entity_descriptor();
        let mut simple_specs = Vec::new();
        // Includes resolved to "search a child collection by a foreign key" — covers both
        // forward has_many WITH options and reverse (incoming belongs_to) includes.
        let mut search_includes: Vec<SearchInclude> = Vec::new();

        for (param_name, opts) in params {
            // Forward: a has_many relation declared on this entity.
            if let Some(rel) = descriptor
                .relations
                .iter()
                .find(|r| matches!(r.kind, RelationKind::HasMany) && r.alias == *param_name)
            {
                if opts.has_options() {
                    // With options we search the child by its FK back to us, which requires
                    // the child to declare belongs_to this entity. Fail loud if it doesn't,
                    // rather than silently dropping the requested options.
                    let fk = child_foreign_key_to(&rel.target, &descriptor.collection, None)
                        .ok_or_else(|| RepoError::InvalidRequest {
                            message: format!(
                                "include '{}' was given options but its target '{}' declares no belongs_to back to '{}'; options require a searchable foreign key",
                                rel.alias, rel.target, descriptor.collection
                            ),
                        })?;
                    search_includes.push(SearchInclude {
                        response_key: rel.alias.clone(),
                        child_collection: rel.target.clone(),
                        foreign_key: fk,
                        opts: opts.clone(),
                    });
                } else {
                    // No options -> materialized relation-set path (atomic Lua).
                    simple_specs.push(FindIncludeSpec {
                        alias: rel.alias.clone(),
                        target_collection: rel.target.clone(),
                        nested: vec![],
                    });
                }
                continue;
            }

            // Reverse: another collection declares belongs_to -> this entity.
            // Grammar: "child_collection" or "child_collection.alias" (disambiguator
            // when one child has multiple foreign keys to this parent).
            let (child_collection, wanted_alias) = match param_name.split_once('.') {
                Some((coll, alias)) => (coll, Some(alias)),
                None => (param_name.as_str(), None),
            };
            let mut candidates =
                incoming_belongs_to(&descriptor.collection, child_collection, wanted_alias);

            match candidates.len() {
                // Unknown include -> skip (forward-compatible, matches forward behavior).
                0 => continue,
                1 => {
                    let inc = candidates.remove(0);
                    // `incoming_belongs_to` only yields foreign-key-bearing relations, so this is
                    // always Some; skip rather than panic if that invariant ever changes.
                    let Some(foreign_key) = inc.foreign_key else { continue };
                    search_includes.push(SearchInclude {
                        response_key: param_name.clone(),
                        child_collection: inc.source_collection,
                        foreign_key,
                        opts: opts.clone(),
                    });
                }
                // Ambiguous bare reference -> fail loud with the valid choices.
                _ => {
                    let choices: Vec<String> = candidates
                        .iter()
                        .map(|c| format!("{}.{}", c.source_collection, c.alias))
                        .collect();
                    return Err(RepoError::InvalidRequest {
                        message: format!(
                            "ambiguous include '{param_name}': '{child_collection}' has multiple foreign keys to '{}'; specify one of [{}]",
                            descriptor.collection,
                            choices.join(", ")
                        ),
                    });
                }
            }
        }

        let mut result = self
            .repo
            .find_with_includes(&mut self.conn, entity_id, simple_specs)
            .await?;

        for inc in search_includes {
            let (children, metadata) = self
                .search_children_by_fk(&inc.child_collection, &inc.foreign_key, entity_id, &inc.opts)
                .await?;
            result.includes.insert(inc.response_key.clone(), children);
            result.relation_metadata.insert(inc.response_key, metadata);
        }

        Ok(result)
    }

    /// Search a child collection for entities whose foreign key equals `parent_id`,
    /// returning the children plus pagination metadata.
    ///
    /// Shared by forward (has_many with options) and reverse (incoming belongs_to)
    /// include resolution — both reduce to "search the child collection by FK".
    async fn search_children_by_fk(
        &mut self,
        child_collection: &str,
        fk: &str,
        parent_id: &str,
        opts: &RelationQueryOptions,
    ) -> Result<(Vec<FindChildResult>, RelationMetadata), RepoError> {
        let child_desc = registry::get_descriptor(child_collection);

        let limit = u64::from(opts.effective_limit());
        let offset = u64::from(opts.offset.unwrap_or(0));

        let sort = match opts.sort.as_ref() {
            Some(s) => {
                let (name, order) = if let Some(f) = s.strip_prefix('-') {
                    (f, search::SortOrder::Desc)
                } else {
                    (s.as_str(), search::SortOrder::Asc)
                };
                // An unknown sort field makes RediSearch error (HTTP 500); reject it up front.
                ensure_known_field(child_desc.as_ref(), child_collection, name)?;
                // Sort by the numeric datetime mirror when one exists for this field.
                let resolved = child_desc
                    .as_ref()
                    .and_then(|d| d.fields.iter().find(|f| f.name == name))
                    .and_then(|f| f.datetime_mirror.clone())
                    .unwrap_or_else(|| name.to_string());
                Some(search::SearchSort { field: resolved, order })
            }
            None => None,
        };

        let mut conditions = vec![search::FilterCondition::tag_eq(fk, parent_id)];
        if let Some(filter) = opts.filter.as_deref() {
            let condition = parse_include_filter(filter)?;
            // An unknown filter field makes RediSearch silently return zero rows; reject it
            // so a typo is a 400, not an empty result indistinguishable from "no matches".
            if let Some(field) = filter_condition_field(&condition) {
                ensure_known_field(child_desc.as_ref(), child_collection, field)?;
            }
            conditions.push(condition);
        }

        // RediSearch `LIMIT <offset> <count>` takes any first-offset, so request the
        // exact window natively (page_size is the count) -- no over-fetch, no slice.
        let search_params = search::SearchParams {
            page: 1,
            page_size: limit.max(1),
            offset_override: Some(offset),
            sort,
            conditions,
            text_query: None,
            raw: None,
        };

        let index_name = format!("{}:{}:idx", self.repo.key_context().prefix, child_collection);
        let search_result: SearchResult<Value> =
            search::execute_search(&mut self.conn, &index_name, &search_params, "").await?;

        let total = search_result.total;
        let children: Vec<FindChildResult> = search_result
            .items
            .into_iter()
            .map(|val| FindChildResult {
                value: val,
                includes: std::collections::HashMap::new(),
            })
            .collect();
        let has_more = total > offset + children.len() as u64;

        Ok((children, RelationMetadata { total: Some(total), has_more: Some(has_more) }))
    }
}

// ============ Query-based Reads ============

impl<T> CollectionHandle<T>
where
    T: SnugomModel + DeserializeOwned + crate::search::SearchEntity,
{
    /// Find first entity matching query.
    ///
    /// Returns `None` if no entity matches.
    pub async fn find_first(
        &mut self,
        query: SearchQuery,
    ) -> Result<Option<T>, RepoError> {
        // Limit to 1 result
        let limited_query = SearchQuery {
            page: Some(1),
            page_size: Some(1),
            ..query
        };
        let result = self.repo.search_with_query(&mut self.conn, limited_query).await?;
        Ok(result.items.into_iter().next())
    }

    /// Find first entity matching query, returning an error if not found.
    ///
    /// This is equivalent to Prisma's `findFirstOrThrow`.
    pub async fn find_first_or_error(
        &mut self,
        query: SearchQuery,
    ) -> Result<T, RepoError> {
        self.find_first(query).await?.ok_or(RepoError::NotFound {
            entity_id: None,
        })
    }

    /// Find all entities matching query (paginated).
    ///
    /// Returns a `SearchResult` containing the matching entities and pagination info.
    pub async fn find_many(
        &mut self,
        query: SearchQuery,
    ) -> Result<SearchResult<T>, RepoError> {
        self.repo.search_with_query(&mut self.conn, query).await
    }

    /// Count entities matching query.
    pub async fn count_where(
        &mut self,
        query: SearchQuery,
    ) -> Result<u64, RepoError> {
        let result = self.find_many(query).await?;
        Ok(result.total)
    }

    /// Check if any entity matches the query.
    pub async fn exists_where(
        &mut self,
        query: SearchQuery,
    ) -> Result<bool, RepoError> {
        let result = self.find_first(query).await?;
        Ok(result.is_some())
    }

    /// Find related entities via a foreign key field.
    ///
    /// Executes an FT.SEARCH query filtering on `fk_field == parent_id`,
    /// with optional limit, sort, filter, and offset from `RelationQueryOptions`.
    /// Returns `RelationData<Vec<T>>` with items, total count, and has_more flag.
    ///
    /// Used by `snugom_find!` for includes with options (limit/sort/filter)
    /// where FT.SEARCH is needed instead of the atomic Lua path.
    pub async fn find_related(
        &mut self,
        fk_field: &str,
        parent_id: &str,
        options: RelationQueryOptions,
    ) -> Result<RelationData<Vec<T>>, RepoError> {
        let limit = options.effective_limit();
        let offset = options.offset.unwrap_or(0);
        let mut filters = vec![format!("{fk_field}:eq:{parent_id}")];
        if let Some(ref extra_filter) = options.filter {
            filters.push(extra_filter.clone());
        }

        let page = if limit > 0 { (offset / limit) + 1 } else { 1 };

        let query = SearchQuery {
            page: Some(page as u64),
            page_size: Some(limit as u64),
            sort_by: options.sort.as_ref().map(|s| {
                if let Some(field) = s.strip_prefix('-') {
                    field.to_string()
                } else {
                    s.clone()
                }
            }),
            sort_order: options.sort.as_ref().map(|s| {
                if s.starts_with('-') {
                    crate::search::SortOrder::Desc
                } else {
                    crate::search::SortOrder::Asc
                }
            }),
            filter: filters,
            q: None,
        };

        let result = self.repo.search_with_query(&mut self.conn, query).await?;
        let total = result.total;
        let has_more = total > (offset as u64 + result.items.len() as u64);

        Ok(RelationData::with_metadata(result.items, total, has_more))
    }
}

// ============ Single Record Writes ============

impl<T> CollectionHandle<T>
where
    T: SnugomModel + DeserializeOwned,
{
    /// Create an entity from a builder.
    ///
    /// Returns the `CreateResult` containing the ID and raw responses.
    /// Use `create_and_get` if you need the full entity back.
    pub async fn create<B>(
        &mut self,
        builder: B,
    ) -> Result<CreateResult, RepoError>
    where
        B: MutationPayloadBuilder,
        B::Entity: EntityMetadata,
    {
        self.repo.create_with_conn(&mut self.conn, builder).await
    }

    /// Create an entity and return the full entity (Prisma-style).
    ///
    /// This is a convenience method that creates the entity and then fetches it.
    pub async fn create_and_get<B>(
        &mut self,
        builder: B,
    ) -> Result<T, RepoError>
    where
        B: MutationPayloadBuilder,
        B::Entity: EntityMetadata,
    {
        self.repo.create_and_get(&mut self.conn, builder).await
    }

    /// Update an entity by ID using a patch builder.
    ///
    /// Returns the raw responses from Redis.
    pub async fn update<B>(
        &mut self,
        builder: B,
    ) -> Result<Vec<Value>, RepoError>
    where
        B: UpdatePatchBuilder,
        B::Entity: EntityMetadata,
        T: EntityMetadata + Serialize,
    {
        self.repo.update_patch_with_conn(&mut self.conn, builder).await
    }

    /// Update an entity and return the full updated entity.
    pub async fn update_and_get<B>(
        &mut self,
        id: &str,
        builder: B,
    ) -> Result<T, RepoError>
    where
        B: UpdatePatchBuilder,
        B::Entity: EntityMetadata,
        T: EntityMetadata + Serialize,
    {
        self.repo.update_patch_with_conn(&mut self.conn, builder).await?;
        self.get_or_error(id).await
    }

    /// Delete an entity by ID.
    pub async fn delete(
        &mut self,
        id: &str,
    ) -> Result<(), RepoError> {
        self.repo.delete_with_conn(&mut self.conn, id, None).await?;
        Ok(())
    }

    /// Delete an entity by ID with optimistic concurrency check.
    pub async fn delete_with_version(
        &mut self,
        id: &str,
        expected_version: u64,
    ) -> Result<(), RepoError> {
        self.repo.delete_with_conn(&mut self.conn, id, Some(expected_version)).await?;
        Ok(())
    }
}

// ============ Bulk Operations ============

impl<T> CollectionHandle<T>
where
    T: SnugomModel + DeserializeOwned,
{
    /// Create multiple entities.
    ///
    /// Returns `BulkCreateResult` containing the count and IDs of created entities.
    pub async fn create_many<B>(
        &mut self,
        builders: Vec<B>,
    ) -> Result<BulkCreateResult, RepoError>
    where
        B: MutationPayloadBuilder,
        B::Entity: EntityMetadata,
    {
        let mut ids = Vec::with_capacity(builders.len());
        let mut responses = Vec::with_capacity(builders.len());

        for builder in builders {
            let result = self.repo.create_with_conn(&mut self.conn, builder).await?;
            ids.push(result.id);
            responses.push(result.responses);
        }

        Ok(BulkCreateResult {
            count: ids.len() as u64,
            ids,
            responses,
        })
    }

    /// Delete multiple entities by IDs.
    ///
    /// Returns the count of successfully deleted entities.
    pub async fn delete_many_by_ids(
        &mut self,
        ids: &[&str],
    ) -> Result<u64, RepoError> {
        let mut deleted = 0u64;
        for id in ids {
            // Try to delete, but don't fail if entity doesn't exist
            match self.repo.delete_with_conn(&mut self.conn, id, None).await {
                Ok(_) => deleted += 1,
                Err(RepoError::NotFound {
                    ..
                }) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(deleted)
    }

    /// Update multiple entities by IDs using a function that generates patches.
    ///
    /// For each ID, calls the patch generator function and applies the resulting patch.
    /// Returns the count of successfully updated entities.
    ///
    /// # Example
    /// ```ignore
    /// let updated = users.update_many_by_ids(&["id1", "id2"], |id| {
    ///     User::patch_builder()
    ///         .entity_id(id)
    ///         .status("inactive".to_string())
    /// }).await?;
    /// ```
    pub async fn update_many_by_ids<B, F>(
        &mut self,
        ids: &[&str],
        patch_fn: F,
    ) -> Result<u64, RepoError>
    where
        B: UpdatePatchBuilder,
        B::Entity: EntityMetadata,
        T: EntityMetadata + Serialize,
        F: Fn(&str) -> B,
    {
        let mut updated = 0u64;
        for id in ids {
            let builder = patch_fn(id);
            match self.repo.update_patch_with_conn(&mut self.conn, builder).await {
                Ok(_) => updated += 1,
                Err(RepoError::NotFound {
                    ..
                }) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(updated)
    }
}

// ============ Query-based Bulk Operations ============

impl<T> CollectionHandle<T>
where
    T: SnugomModel + DeserializeOwned + crate::search::SearchEntity,
{
    /// Delete all entities matching the query.
    ///
    /// Returns the count of deleted entities.
    ///
    /// Note: This performs a search first to find matching IDs, then deletes them.
    /// For large result sets, consider pagination.
    pub async fn delete_many(
        &mut self,
        query: SearchQuery,
    ) -> Result<u64, RepoError> {
        // First, find all matching entities to get their IDs
        let result = self.repo.search_with_query(&mut self.conn, query).await?;

        // Delete each entity by ID
        let mut deleted = 0u64;
        for item in result.items {
            let id = T::get_id(&item);
            match self.repo.delete_with_conn(&mut self.conn, &id, None).await {
                Ok(_) => deleted += 1,
                Err(RepoError::NotFound {
                    ..
                }) => {}
                Err(e) => return Err(e),
            }
        }

        Ok(deleted)
    }

    /// Update all entities matching the query using a patch generator function.
    ///
    /// Returns the count of successfully updated entities.
    ///
    /// Note: This performs a search first to find matching entities, then updates them.
    /// For large result sets, consider pagination.
    ///
    /// # Example
    /// ```ignore
    /// // Deactivate all users who haven't logged in recently
    /// let query = SearchQuery::new().filter("last_login".lt("2024-01-01"));
    /// let updated = users.update_many(query, |id| {
    ///     User::patch_builder()
    ///         .entity_id(id)
    ///         .status("inactive".to_string())
    /// }).await?;
    /// ```
    pub async fn update_many<B, F>(
        &mut self,
        query: SearchQuery,
        patch_fn: F,
    ) -> Result<u64, RepoError>
    where
        B: UpdatePatchBuilder,
        B::Entity: EntityMetadata,
        T: EntityMetadata + Serialize,
        F: Fn(&str) -> B,
    {
        // First, find all matching entities to get their IDs
        let result = self.repo.search_with_query(&mut self.conn, query).await?;

        // Update each entity by ID
        let mut updated = 0u64;
        for item in result.items {
            let id = T::get_id(&item);
            let builder = patch_fn(&id);
            match self.repo.update_patch_with_conn(&mut self.conn, builder).await {
                Ok(_) => updated += 1,
                Err(RepoError::NotFound {
                    ..
                }) => {}
                Err(e) => return Err(e),
            }
        }

        Ok(updated)
    }
}

// ============ Upsert Operations ============

impl<T> CollectionHandle<T>
where
    T: SnugomModel + DeserializeOwned + EntityMetadata + Serialize,
{
    /// Upsert: creates if entity doesn't exist, updates if it does.
    ///
    /// This operation is atomic - the existence check and mutation happen in a single
    /// Redis Lua script call, preventing race conditions.
    pub async fn upsert<C, U>(
        &mut self,
        create_builder: C,
        update_builder: U,
    ) -> Result<UpsertResult, RepoError>
    where
        C: MutationPayloadBuilder,
        C::Entity: EntityMetadata,
        U: UpdatePatchBuilder,
        U::Entity: EntityMetadata,
    {
        self.repo.upsert(&mut self.conn, create_builder, update_builder).await
    }

    /// Get or create: returns existing entity or creates it if it doesn't exist.
    ///
    /// This operation is atomic - the existence check and create happen in a single
    /// Redis Lua script call, preventing race conditions.
    ///
    /// Unlike `upsert`, if the entity exists it is returned as-is without modification.
    pub async fn get_or_create<C>(
        &mut self,
        create_builder: C,
    ) -> Result<GetOrCreateResult<T>, RepoError>
    where
        C: MutationPayloadBuilder,
        C::Entity: EntityMetadata,
    {
        self.repo.get_or_create(&mut self.conn, create_builder).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests would require a real Redis connection
    // They serve as documentation of the expected API

    #[test]
    fn test_bulk_create_result_structure() {
        let result = BulkCreateResult {
            count: 3,
            ids: vec!["id1".to_string(), "id2".to_string(), "id3".to_string()],
            responses: vec![],
        };
        assert_eq!(result.count, 3);
        assert_eq!(result.ids.len(), 3);
    }

    #[test]
    fn parse_include_filter_eq() {
        match parse_include_filter("role:eq:admin").expect("valid eq filter") {
            search::FilterCondition::TagEquals { field, values } => {
                assert_eq!(field, "role");
                assert_eq!(values, vec!["admin".to_string()]);
            }
            other => panic!("expected TagEquals, got {other:?}"),
        }
    }

    #[test]
    fn parse_include_filter_bool() {
        match parse_include_filter("active:bool:true").expect("valid bool filter") {
            search::FilterCondition::BooleanEquals { field, value } => {
                assert_eq!(field, "active");
                assert!(value);
            }
            other => panic!("expected BooleanEquals, got {other:?}"),
        }
        assert!(parse_include_filter("active:bool:notabool").is_err());
    }

    #[test]
    fn parse_include_filter_range_uses_dotdot() {
        match parse_include_filter("score:range:10..100").expect("valid range filter") {
            search::FilterCondition::NumericRange { field, min, max } => {
                assert_eq!(field, "score");
                assert_eq!(min, Some(10.0));
                assert_eq!(max, Some(100.0));
            }
            other => panic!("expected NumericRange, got {other:?}"),
        }
    }

    #[test]
    fn parse_include_filter_range_open_ended_bounds() {
        match parse_include_filter("score:range:10..").expect("open upper bound") {
            search::FilterCondition::NumericRange { min, max, .. } => {
                assert_eq!(min, Some(10.0));
                assert_eq!(max, None);
            }
            other => panic!("expected NumericRange, got {other:?}"),
        }
        match parse_include_filter("score:range:..100").expect("open lower bound") {
            search::FilterCondition::NumericRange { min, max, .. } => {
                assert_eq!(min, None);
                assert_eq!(max, Some(100.0));
            }
            other => panic!("expected NumericRange, got {other:?}"),
        }
    }

    #[test]
    fn parse_include_filter_is_case_insensitive_and_accepts_aliases() {
        // Operator is lowercased; `boolean` aliases `bool`; surrounding whitespace trimmed.
        assert!(matches!(
            parse_include_filter("role:EQ:admin").expect("uppercase op"),
            search::FilterCondition::TagEquals { .. }
        ));
        assert!(matches!(
            parse_include_filter("active:Boolean:true").expect("boolean alias"),
            search::FilterCondition::BooleanEquals { value: true, .. }
        ));
        // Fully-open range (`..`) parses to no bounds.
        match parse_include_filter("score:range:..").expect("open range") {
            search::FilterCondition::NumericRange { min, max, .. } => {
                assert_eq!(min, None);
                assert_eq!(max, None);
            }
            other => panic!("expected NumericRange, got {other:?}"),
        }
    }

    #[test]
    fn parse_include_filter_rejects_malformed() {
        // Missing the value segment.
        assert!(parse_include_filter("role:eq").is_err());
        // Unknown operator.
        assert!(parse_include_filter("role:like:admin").is_err());
        // Range with a non-numeric bound.
        assert!(parse_include_filter("score:range:low..high").is_err());
        // Range without the `..` separator (e.g. the old comma form).
        assert!(parse_include_filter("score:range:10,100").is_err());
    }
}
