use crate::search::SortOrder;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Placeholder metadata structures emitted by the derive macro in later phases.
#[derive(Debug, Default, Clone)]
pub struct EntityDescriptor {
    pub service: String,
    pub collection: String,
    pub version: u32,
    pub id_field: Option<String>,
    pub relations: Vec<RelationDescriptor>,
    pub fields: Vec<FieldDescriptor>,
    pub derived_id: Option<DerivedIdDescriptor>,
    /// Unique constraints on this entity (single-field and compound)
    pub unique_constraints: Vec<UniqueConstraintDescriptor>,
}

#[derive(Debug, Clone)]
pub struct RelationDescriptor {
    pub alias: String,
    pub target: String,
    pub target_service: Option<String>,
    pub kind: RelationKind,
    pub cascade: CascadePolicy,
    pub foreign_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DerivedIdDescriptor {
    pub separator: String,
    pub components: Vec<String>,
}

/// Describes a unique constraint on one or more fields.
///
/// Single-field constraints are defined with `#[snugom(unique)]` on a field.
/// Compound constraints (multiple fields) are defined with `#[snugom(unique_together = ["f1", "f2"])]`
/// at the entity level.
///
/// # Examples
///
/// ```text
/// // Single-field unique
/// #[snugom(unique)]
/// pub name: String,
///
/// // Case-insensitive unique
/// #[snugom(unique(case_insensitive))]
/// pub slug: String,
///
/// // Compound unique (at entity level)
/// #[snugom(unique_together = ["tenant_id", "name"])]
/// pub struct Project { ... }
/// ```
#[derive(Debug, Clone)]
pub struct UniqueConstraintDescriptor {
    /// Field names in the constraint. Single-field has one element, compound has multiple.
    pub fields: Vec<String>,
    /// Whether string comparisons ignore case (e.g., "Foo" == "foo")
    pub case_insensitive: bool,
}

impl UniqueConstraintDescriptor {
    /// Create a single-field unique constraint
    pub fn single(field: impl Into<String>, case_insensitive: bool) -> Self {
        Self {
            fields: vec![field.into()],
            case_insensitive,
        }
    }

    /// Create a compound unique constraint over multiple fields
    pub fn compound(fields: Vec<String>, case_insensitive: bool) -> Self {
        Self {
            fields,
            case_insensitive,
        }
    }

    /// Returns true if this is a compound constraint (multiple fields)
    pub fn is_compound(&self) -> bool {
        self.fields.len() > 1
    }
}

#[derive(Debug, Clone, Copy)]
#[derive(Default)]
pub enum RelationKind {
    #[default]
    HasMany,
    ManyToMany,
    BelongsTo,
}


#[derive(Debug, Clone, Copy)]
#[derive(Default)]
pub enum CascadePolicy {
    Delete,
    Detach,
    #[default]
    None,
}


pub trait EntityMetadata {
    /// Whether this entity has at least one indexed field (filterable or sortable).
    /// Entities used in bundles must have indexed fields to support search operations.
    const HAS_INDEXED_FIELDS: bool;

    fn entity_descriptor() -> EntityDescriptor;
    fn ensure_registered();
}

/// Trait for entities registered with SnugOM.
///
/// This trait is automatically implemented by `#[derive(SnugomEntity)]`.
/// It provides the service and collection names used for Redis key generation.
pub trait SnugomModel: EntityMetadata {
    /// The service name this entity belongs to
    const SERVICE: &'static str;

    /// The collection name for this entity (auto-pluralized from struct name or explicit override)
    const COLLECTION: &'static str;

    /// Get the ID of this entity instance.
    ///
    /// This is used by collection operations like `delete_many` that need to extract
    /// the ID from fetched entities.
    fn get_id(&self) -> String;
}

#[derive(Debug, Clone, Default)]
pub struct FieldDescriptor {
    pub name: String,
    pub optional: bool,
    pub is_id: bool,
    pub validations: Vec<ValidationDescriptor>,
    pub datetime_mirror: Option<String>,
    pub auto_updated: bool,
    pub auto_created: bool,
    pub field_type: FieldType,
    pub element_type: Option<FieldType>,
    /// True if this field is a relation Vec (has_many, many_to_many) that defaults to empty
    pub is_relation_vec: bool,
    /// When true, normalize enum values to just their discriminant (variant name) at write time.
    /// This handles enums with associated data that serialize to objects (e.g., {"swiss": {"rounds": 6}})
    /// which RediSearch cannot index as TAG fields. The full enum value is preserved in the document,
    /// but the indexed value becomes just the variant name string (e.g., "swiss").
    pub normalize_enum_tag: bool,
}

pub type DatetimeMirrors = Vec<DatetimeMirrorValue>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum FieldType {
    String,
    Number,
    Boolean,
    Array,
    #[default]
    Object,
    DateTime,
}


#[derive(Debug, Clone, Copy)]
pub enum ValidationScope {
    Field,
    EachElement,
}

#[derive(Debug, Clone)]
pub struct ValidationDescriptor {
    pub scope: ValidationScope,
    pub rule: ValidationRule,
}

#[derive(Debug, Clone)]
pub enum ValidationRule {
    Length {
        min: Option<usize>,
        max: Option<usize>,
    },
    Range {
        min: Option<String>,
        max: Option<String>,
    },
    Regex {
        pattern: String,
    },
    Enum {
        allowed: Vec<String>,
        case_insensitive: bool,
    },
    Email,
    Url,
    Uuid,
    RequiredIf {
        expr: String,
    },
    ForbiddenIf {
        expr: String,
    },
    /// Unique constraint on a field. Duplicate values are rejected at creation/update time.
    Unique {
        /// Whether string comparisons ignore case
        case_insensitive: bool,
    },
    Custom {
        path: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct DatetimeMirrorValue {
    pub field: String,
    pub mirror_field: String,
    pub value: Option<i64>,
}

impl DatetimeMirrorValue {
    pub fn new(field: impl Into<String>, mirror_field: impl Into<String>, value: Option<i64>) -> Self {
        Self {
            field: field.into(),
            mirror_field: mirror_field.into(),
            value,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// RelationState<T> - Wrapper for relation fields that tracks loading status
// ═══════════════════════════════════════════════════════════════════════════════

/// Represents the state of a relation field on an entity.
///
/// Relations can be in one of two states:
/// - `NotLoaded`: The relation was not requested/fetched. The field will be
///   omitted from JSON serialization.
/// - `Loaded(T)`: The relation was fetched and contains the data (which may be
///   empty, e.g., `Loaded(vec![])` for a has_many with no children).
///
/// # JSON Serialization
///
/// - `NotLoaded` → field is **absent** from JSON output
/// - `Loaded(vec![...])` → `"field": [...]`
/// - `Loaded(vec![])` → `"field": []`
///
/// # Example
///
/// ```text
/// #[snugom_model]
/// pub struct Guild {
///     #[id]
///     pub guild_id: String,
///     pub name: String,
///
///     #[relation]
///     pub members: Vec<GuildMember>,  // Becomes RelationState<Vec<GuildMember>>
/// }
///
/// // Without include - members not in JSON
/// let guild = repo.get(conn, "g_123").await?;
/// // guild.members == NotLoaded
/// // JSON: { "guild_id": "g_123", "name": "..." }
///
/// // With include - members in JSON
/// let guild = repo.get(conn, "g_123").include_members().await?;
/// // guild.members == Loaded([...])
/// // JSON: { "guild_id": "g_123", "name": "...", "members": [...] }
/// ```
#[derive(Debug, Clone, PartialEq)]
#[derive(Default)]
pub enum RelationState<T> {
    /// The relation was not requested/loaded. Will be omitted from JSON.
    #[default]
    NotLoaded,
    /// The relation was fetched. Contains the data (may be empty).
    Loaded(T),
}


impl<T> RelationState<T> {
    /// Returns `true` if the relation has been loaded.
    #[inline]
    pub fn is_loaded(&self) -> bool {
        matches!(self, RelationState::Loaded(_))
    }

    /// Returns `true` if the relation has not been loaded.
    /// Used by serde's `skip_serializing_if` to omit unloaded relations from JSON.
    #[inline]
    pub fn is_not_loaded(&self) -> bool {
        matches!(self, RelationState::NotLoaded)
    }

    /// Returns a reference to the loaded data, or `None` if not loaded.
    #[inline]
    pub fn as_loaded(&self) -> Option<&T> {
        match self {
            RelationState::Loaded(v) => Some(v),
            RelationState::NotLoaded => None,
        }
    }

    /// Returns a mutable reference to the loaded data, or `None` if not loaded.
    #[inline]
    pub fn as_loaded_mut(&mut self) -> Option<&mut T> {
        match self {
            RelationState::Loaded(v) => Some(v),
            RelationState::NotLoaded => None,
        }
    }

    /// Consumes self and returns the loaded data, or `None` if not loaded.
    #[inline]
    pub fn into_loaded(self) -> Option<T> {
        match self {
            RelationState::Loaded(v) => Some(v),
            RelationState::NotLoaded => None,
        }
    }

    /// Consumes self and returns the loaded data, or a default value if not loaded.
    #[inline]
    pub fn unwrap_or(self, default: T) -> T {
        match self {
            RelationState::Loaded(v) => v,
            RelationState::NotLoaded => default,
        }
    }

    /// Consumes self and returns the loaded data, or computes a default if not loaded.
    #[inline]
    pub fn unwrap_or_else<F: FnOnce() -> T>(self, f: F) -> T {
        match self {
            RelationState::Loaded(v) => v,
            RelationState::NotLoaded => f(),
        }
    }

    /// Maps a `RelationState<T>` to `RelationState<U>` by applying a function.
    #[inline]
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> RelationState<U> {
        match self {
            RelationState::Loaded(v) => RelationState::Loaded(f(v)),
            RelationState::NotLoaded => RelationState::NotLoaded,
        }
    }

    /// Converts from `&RelationState<T>` to `RelationState<&T>`.
    #[inline]
    pub fn as_ref(&self) -> RelationState<&T> {
        match self {
            RelationState::Loaded(v) => RelationState::Loaded(v),
            RelationState::NotLoaded => RelationState::NotLoaded,
        }
    }
}

impl<T: Default> RelationState<T> {
    /// Consumes self and returns the loaded data, or `T::default()` if not loaded.
    #[inline]
    pub fn unwrap_or_default(self) -> T {
        match self {
            RelationState::Loaded(v) => v,
            RelationState::NotLoaded => T::default(),
        }
    }
}

// Serde implementation: Loaded serializes inner value, NotLoaded is skipped via skip_serializing_if
impl<T: Serialize> Serialize for RelationState<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            // NotLoaded should be skipped by skip_serializing_if, but if called directly,
            // serialize as null (though this shouldn't happen in practice)
            RelationState::NotLoaded => serializer.serialize_none(),
            RelationState::Loaded(value) => value.serialize(serializer),
        }
    }
}

// Deserialize: Missing field = NotLoaded (via #[serde(default)]), present = Loaded
impl<'de, T: Deserialize<'de>> Deserialize<'de> for RelationState<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // When the field is present in JSON, deserialize as Loaded
        T::deserialize(deserializer).map(RelationState::Loaded)
    }
}

/// Metadata about a loaded relation, including pagination info.
/// Used when fetching relations with limits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelationData<T> {
    /// The loaded items
    pub items: T,
    /// Total count of related entities (if known)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Whether more items exist beyond what was fetched
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

impl<T> RelationData<T> {
    /// Create new relation data with just items (no pagination metadata)
    pub fn new(items: T) -> Self {
        Self {
            items,
            total: None,
            has_more: None,
        }
    }

    /// Create new relation data with pagination metadata
    pub fn with_metadata(items: T, total: u64, has_more: bool) -> Self {
        Self {
            items,
            total: Some(total),
            has_more: Some(has_more),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// RelationQueryOptions - Query options for nested relation fetching
// ═══════════════════════════════════════════════════════════════════════════════

/// Options for controlling how related entities are fetched.
///
/// Used with the `?include=relation(options)` syntax to control:
/// - How many related entities to return (limit)
/// - What order to return them in (sort)
/// - Which ones to include (filter)
/// - Pagination offset
///
/// # Example
///
/// ```
/// use snugom::types::RelationQueryOptions;
///
/// // Parse from URL: ?include=members(limit:3,sort:-joined_at)
/// let options = RelationQueryOptions::default()
///     .with_limit(3)
///     .with_sort("-joined_at");
///
/// // Use programmatically
/// let options = RelationQueryOptions::default()
///     .with_limit(10)
///     .with_sort("role")
///     .with_filter("role:eq:admin");
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RelationQueryOptions {
    /// Maximum number of related entities to fetch
    pub limit: Option<u32>,
    /// Sort specification (e.g., "-joined_at" for desc, "name" for asc)
    pub sort: Option<String>,
    /// Filter expression (e.g., "role:eq:admin")
    pub filter: Option<String>,
    /// Offset for pagination (combine with limit)
    pub offset: Option<u32>,
}

/// Default limit for relations to prevent accidental large fetches
pub const DEFAULT_RELATION_LIMIT: u32 = 100;
/// Maximum allowed limit for relations
pub const MAX_RELATION_LIMIT: u32 = 1000;

impl RelationQueryOptions {
    /// Create new empty options
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum number of items to return
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit.min(MAX_RELATION_LIMIT));
        self
    }

    /// Set the sort specification
    /// Use "-field" for descending, "field" for ascending
    pub fn with_sort(mut self, sort: impl Into<String>) -> Self {
        self.sort = Some(sort.into());
        self
    }

    /// Set a filter expression
    /// Format: "field:op:value" (e.g., "role:eq:admin", "status:in:pending,approved")
    pub fn with_filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = Some(filter.into());
        self
    }

    /// Set the pagination offset
    pub fn with_offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Get the effective limit, applying defaults and caps
    pub fn effective_limit(&self) -> u32 {
        self.limit.unwrap_or(DEFAULT_RELATION_LIMIT).min(MAX_RELATION_LIMIT)
    }

    /// Check if any options are set
    pub fn has_options(&self) -> bool {
        self.limit.is_some() || self.sort.is_some() || self.filter.is_some() || self.offset.is_some()
    }

    /// Parse sort specification into field and direction
    pub fn parse_sort(&self) -> Option<(&str, SortOrder)> {
        self.sort.as_ref().map(|s| {
            if let Some(field) = s.strip_prefix('-') {
                (field, SortOrder::Desc)
            } else {
                (s.as_str(), SortOrder::Asc)
            }
        })
    }
}

