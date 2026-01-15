//! Schema types for representing parsed SnugomEntity structures.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Complete schema for an entity, suitable for snapshot serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySchema {
    /// Name of the entity struct (e.g., "User")
    pub entity: String,

    /// Collection name from entity attributes (e.g., "users")
    /// This is populated from #[snugom(collection = "...")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,

    /// Schema version from #[snugom(schema = N)]
    pub schema: u32,

    /// All fields in the entity
    pub fields: Vec<FieldInfo>,

    /// Relations defined on this entity
    pub relations: Vec<RelationInfo>,

    /// Unique constraints (compound)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unique_constraints: Vec<UniqueConstraint>,

    /// Index fields (derived from filterable/sortable)
    pub indexes: Vec<IndexInfo>,

    /// When this snapshot was generated
    pub generated_at: DateTime<Utc>,

    /// Source file path (relative to project root)
    pub source_file: String,

    /// Line number where the struct is defined
    pub source_line: usize,
}

impl EntitySchema {
    /// Create a new entity schema with default values
    pub fn new(entity: String, source_file: String, source_line: usize) -> Self {
        Self {
            entity,
            collection: None,
            schema: 1,
            fields: Vec::new(),
            relations: Vec::new(),
            unique_constraints: Vec::new(),
            indexes: Vec::new(),
            generated_at: Utc::now(),
            source_file,
            source_line,
        }
    }

    /// Generate a snapshot filename for this entity and version
    pub fn snapshot_filename(&self) -> String {
        let entity_snake = to_snake_case(&self.entity);
        format!("{entity_snake}_v{}.json", self.schema)
    }
}

/// Information about a single field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldInfo {
    /// Field name
    pub name: String,

    /// Rust type as string (e.g., "String", "Option<String>", "Vec<String>")
    #[serde(rename = "type")]
    pub field_type: String,

    /// Whether this is the ID field
    #[serde(default, skip_serializing_if = "is_false")]
    pub id: bool,

    /// Filterable type if set
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filterable: Option<FilterableType>,

    /// Whether this field is sortable
    #[serde(default, skip_serializing_if = "is_false")]
    pub sortable: bool,

    /// Unique constraint on this field
    #[serde(default, skip_serializing_if = "is_false")]
    pub unique: bool,

    /// Case-insensitive unique
    #[serde(default, skip_serializing_if = "is_false")]
    pub unique_case_insensitive: bool,

    /// DateTime storage format
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datetime_format: Option<DateTimeFormat>,

    /// Serde default value if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serde_default: Option<String>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl FieldInfo {
    pub fn new(name: String, field_type: String) -> Self {
        Self {
            name,
            field_type,
            id: false,
            filterable: None,
            sortable: false,
            unique: false,
            unique_case_insensitive: false,
            datetime_format: None,
            serde_default: None,
        }
    }
}

/// Filterable field type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterableType {
    Tag,
    Text,
    Numeric,
    Geo,
}

impl std::fmt::Display for FilterableType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterableType::Tag => write!(f, "tag"),
            FilterableType::Text => write!(f, "text"),
            FilterableType::Numeric => write!(f, "numeric"),
            FilterableType::Geo => write!(f, "geo"),
        }
    }
}

/// DateTime storage format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateTimeFormat {
    EpochMillis,
    EpochSecs,
    Iso8601,
}

/// Information about a relation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationInfo {
    /// Field name that holds the relation
    pub field: String,

    /// Target collection name
    pub target: String,

    /// Relation kind
    pub kind: RelationKind,

    /// Cascade strategy
    #[serde(default)]
    pub cascade: CascadeStrategy,
}

/// Relation kind
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum RelationKind {
    #[default]
    BelongsTo,
    HasMany,
    ManyToMany,
}

/// Cascade strategy for relations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum CascadeStrategy {
    #[default]
    Detach,
    Delete,
    Restrict,
}

/// Compound unique constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniqueConstraint {
    /// Fields that make up the compound key
    pub fields: Vec<String>,

    /// Whether comparison is case-insensitive
    #[serde(default)]
    pub case_insensitive: bool,
}

/// Index information (derived from filterable/sortable fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexInfo {
    /// Field name
    pub field: String,

    /// Index type
    #[serde(rename = "type")]
    pub index_type: IndexType,
}

/// Index type for RediSearch
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IndexType {
    Tag,
    Text,
    Numeric,
    Geo,
}

impl From<FilterableType> for IndexType {
    fn from(ft: FilterableType) -> Self {
        match ft {
            FilterableType::Tag => IndexType::Tag,
            FilterableType::Text => IndexType::Text,
            FilterableType::Numeric => IndexType::Numeric,
            FilterableType::Geo => IndexType::Geo,
        }
    }
}

/// Field type classification for determining default behaviors
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum FieldType {
    String,
    OptionalString,
    Integer,
    Float,
    Bool,
    DateTime,
    OptionalDateTime,
    Vec(Box<FieldType>),
    Option(Box<FieldType>),
    Other(String),
}

#[allow(dead_code)]
impl FieldType {
    /// Parse a Rust type string into a FieldType
    pub fn parse(type_str: &str) -> Self {
        let trimmed = type_str.trim();

        if trimmed == "String" {
            return FieldType::String;
        }

        if trimmed.starts_with("Option<") && trimmed.ends_with('>') {
            let inner = &trimmed[7..trimmed.len() - 1];
            if inner == "String" {
                return FieldType::OptionalString;
            }
            if inner.contains("DateTime") {
                return FieldType::OptionalDateTime;
            }
            return FieldType::Option(Box::new(FieldType::parse(inner)));
        }

        if trimmed.starts_with("Vec<") && trimmed.ends_with('>') {
            let inner = &trimmed[4..trimmed.len() - 1];
            return FieldType::Vec(Box::new(FieldType::parse(inner)));
        }

        if trimmed.contains("DateTime") {
            return FieldType::DateTime;
        }

        match trimmed {
            "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128" | "usize" => {
                FieldType::Integer
            }
            "f32" | "f64" => FieldType::Float,
            "bool" => FieldType::Bool,
            other => FieldType::Other(other.to_string()),
        }
    }

    /// Get the JSON default value for this type
    pub fn default_json_value(&self) -> serde_json::Value {
        match self {
            FieldType::String => serde_json::Value::String(String::new()),
            FieldType::OptionalString | FieldType::Option(_) | FieldType::OptionalDateTime => serde_json::Value::Null,
            FieldType::Integer => serde_json::json!(0),
            FieldType::Float => serde_json::json!(0.0),
            FieldType::Bool => serde_json::Value::Bool(false),
            FieldType::DateTime => serde_json::json!(0), // epoch millis
            FieldType::Vec(_) => serde_json::json!([]),
            FieldType::Other(_) => serde_json::Value::Null,
        }
    }

    /// Check if this type is optional
    pub fn is_optional(&self) -> bool {
        matches!(
            self,
            FieldType::OptionalString | FieldType::Option(_) | FieldType::OptionalDateTime
        )
    }
}

/// Convert PascalCase to snake_case
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_type_parse() {
        assert_eq!(FieldType::parse("String"), FieldType::String);
        assert_eq!(FieldType::parse("Option<String>"), FieldType::OptionalString);
        assert_eq!(FieldType::parse("i32"), FieldType::Integer);
        assert_eq!(FieldType::parse("f64"), FieldType::Float);
        assert_eq!(FieldType::parse("bool"), FieldType::Bool);
        assert!(matches!(FieldType::parse("DateTime<Utc>"), FieldType::DateTime));
        assert!(matches!(FieldType::parse("Vec<String>"), FieldType::Vec(_)));
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("User"), "user");
        assert_eq!(to_snake_case("UserProfile"), "user_profile");
        assert_eq!(to_snake_case("HTTPRequest"), "h_t_t_p_request");
    }

    #[test]
    fn test_snapshot_filename() {
        let schema = EntitySchema::new("UserProfile".to_string(), "src/user.rs".to_string(), 10);
        assert_eq!(schema.snapshot_filename(), "user_profile_v1.json");
    }
}
