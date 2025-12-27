//! Comprehensive tests for the SnugomEntity derive macro's search functionality.
//!
//! These tests cover all entries in the capability table from the implementation plan:
//! - Numeric fields (entries 1-15)
//! - Boolean fields (entries 16-19)
//! - Enum fields (entries 20-25)
//! - String TEXT fields (entries 26-32)
//! - String TAG fields (entries 33-39)
//! - DateTime fields (entries 40-48)
//! - Array fields (entries 49-53)
//! - Geographic fields (entries 54-56)
//! - Combined scenarios (entries 57-65)
//! - Error cases (entries 66-70) - tested via trybuild

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use snugom::search::{IndexFieldType, SearchEntity, SortOrder};
use snugom::{SnugomEntity, bundle};

// =============================================================================
// Test Entities - Numeric Fields (Entries 1-15)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct NumericEntity {
    #[snugom(id)]
    pub id: String,

    /// Entry 1-2: Filter by exact number / range
    #[snugom(filterable)]
    pub count: u32,

    /// Entry 3, 6: Sort by number (no filter)
    #[snugom(sortable)]
    pub score: u32,

    /// Entry 4: Filter AND sort by number
    #[snugom(filterable, sortable)]
    pub level: u32,

    /// Entry 5: Index for internal use only
    #[snugom(indexed)]
    pub internal_score: u32,

    /// Entry 7-8: Filter/sort with alias
    #[snugom(filterable, sortable, alias = "num")]
    pub raw_count: u32,

    /// Entry 9: Filter signed integer
    #[snugom(filterable)]
    pub signed_value: i64,

    /// Entry 10-11: Filter + sort float
    #[snugom(filterable, sortable)]
    pub rating: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct OptionalNumericEntity {
    #[snugom(id)]
    pub id: String,

    /// Entry 12: Filter optional number
    #[snugom(filterable)]
    pub level: Option<u32>,

    /// Entry 13: Sort optional number
    #[snugom(sortable)]
    pub rank: Option<u32>,

    /// Entry 14: Filter + sort optional
    #[snugom(filterable, sortable)]
    pub xp: Option<u32>,

    /// Entry 15: Filter optional with alias
    #[snugom(filterable, alias = "pts")]
    pub points: Option<i64>,
}

// =============================================================================
// Test Entities - Boolean Fields (Entries 16-19)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct BooleanEntity {
    #[snugom(id)]
    pub id: String,

    /// Entry 16: Filter by boolean
    #[snugom(filterable)]
    pub active: bool,

    /// Entry 17: Filter boolean with alias
    #[snugom(filterable, alias = "is_enabled")]
    pub enabled: bool,

    /// Entry 18: Index boolean (internal)
    #[snugom(indexed)]
    pub internal_flag: bool,

    /// Entry 19: Filter optional boolean
    #[snugom(filterable)]
    pub verified: Option<bool>,
}

// =============================================================================
// Test Entities - Enum Fields (Entries 20-25)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TestStatus {
    #[default]
    Active,
    Pending,
    Inactive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    #[default]
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct EnumEntity {
    #[snugom(id)]
    pub id: String,

    /// Entry 20-21: Filter by enum value / multiple values
    #[snugom(filterable)]
    pub status: TestStatus,

    /// Entry 22: Filter enum with alias
    #[snugom(filterable, alias = "state")]
    pub current_state: TestStatus,

    /// Entry 23: Index enum (internal)
    #[snugom(indexed)]
    pub internal_status: TestStatus,

    /// Entry 24: Filter optional enum
    #[snugom(filterable)]
    pub role: Option<TestStatus>,

    /// Entry 25: Sort by enum (alphabetic)
    #[snugom(filterable, sortable)]
    pub priority: Priority,
}

// =============================================================================
// Test Entities - String TEXT Fields (Entries 26-32)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct TextSearchEntity {
    #[snugom(id)]
    pub id: String,

    /// Entry 26: Full-text search only
    #[snugom(searchable)]
    pub name: String,

    /// Entry 27: Full-text + sortable
    #[snugom(searchable, sortable)]
    pub title: String,

    /// Entry 30: Full-text optional string
    #[snugom(searchable)]
    pub bio: Option<String>,

    /// Entry 31: Full-text + sort optional
    #[snugom(searchable, sortable)]
    pub subtitle: Option<String>,

    /// Entry 32: Index text (internal)
    #[snugom(indexed(text))]
    pub internal_text: String,
}

// =============================================================================
// Test Entities - String TAG Fields (Entries 33-39)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct TagStringEntity {
    #[snugom(id)]
    pub id: String,

    /// Entry 33-34: Filter exact string / multiple
    #[snugom(filterable(tag))]
    pub slug: String,

    /// Entry 35, 39: Filter + sort exact string
    #[snugom(filterable(tag), sortable)]
    pub region: String,

    /// Entry 36: Filter exact with alias
    #[snugom(filterable(tag), alias = "ref")]
    pub reference: String,

    /// Entry 37: Index exact string (internal)
    #[snugom(indexed(tag))]
    pub internal_key: String,

    /// Entry 38: Filter optional exact string
    #[snugom(filterable(tag))]
    pub external_id: Option<String>,
}

// =============================================================================
// Test Entities - DateTime Fields (Entries 40-48)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1, default_sort = "-created_at")]
pub struct DateTimeEntity {
    #[snugom(id)]
    pub id: String,

    /// Entry 40: Filter by date range
    #[snugom(datetime(epoch_millis), filterable)]
    pub expires_at: DateTime<Utc>,

    /// Entry 41: Sort by date
    #[snugom(datetime(epoch_millis), sortable)]
    pub scheduled_at: DateTime<Utc>,

    /// Entry 42: Filter + sort date
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    pub modified_at: DateTime<Utc>,

    /// Entry 43: Auto-created timestamp
    #[snugom(datetime(epoch_millis), created_at, filterable, sortable)]
    pub created_at: DateTime<Utc>,

    /// Entry 44: Auto-updated timestamp
    #[snugom(datetime(epoch_millis), updated_at, filterable, sortable)]
    pub updated_at: DateTime<Utc>,

    /// Entry 45: Date with alias
    #[snugom(datetime(epoch_millis), filterable, alias = "date")]
    pub event_date: DateTime<Utc>,

    /// Entry 46: Optional date filter
    #[snugom(datetime(epoch_millis), filterable)]
    pub published_at: Option<DateTime<Utc>>,

    /// Entry 47: Optional date sort
    #[snugom(datetime(epoch_millis), sortable)]
    pub archived_at: Option<DateTime<Utc>>,

    /// Entry 48: Date index only (internal)
    #[snugom(datetime(epoch_millis), indexed)]
    pub internal_ts: DateTime<Utc>,
}

// =============================================================================
// Test Entities - Array Fields (Entries 49-53)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct ArrayEntity {
    #[snugom(id)]
    pub id: String,

    /// Entry 49-50: Filter by tag in array / multiple tags
    #[snugom(filterable)]
    pub tags: Vec<String>,

    /// Entry 51: Filter array with alias
    #[snugom(filterable, alias = "labels")]
    pub raw_tags: Vec<String>,

    /// Entry 52: Index array (internal)
    #[snugom(indexed)]
    pub internal_tags: Vec<String>,
}

// =============================================================================
// Test Entities - Geographic Fields (Entries 54-56)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct GeoEntity {
    #[snugom(id)]
    pub id: String,

    /// Entry 54: Filter by location radius
    #[snugom(filterable(geo))]
    pub location: String,

    /// Entry 55: Geo with alias
    #[snugom(filterable(geo), alias = "coords")]
    pub position: String,

    /// Entry 56: Index geo (internal)
    #[snugom(indexed(geo))]
    pub internal_geo: String,
}

// =============================================================================
// Test Entities - Combined/Complex Scenarios (Entries 57-65)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1, default_sort = "-created_at")]
pub struct CombinedEntity {
    #[snugom(id)]
    pub id: String,

    /// Entry 57: Multiple fields in full-text (searchable 1)
    #[snugom(searchable, validate(length(min = 1, max = 200)))]
    pub name: String,

    /// Entry 57: Multiple fields in full-text (searchable 2)
    #[snugom(searchable)]
    pub description: Option<String>,

    /// Entry 58-59: Default sort field
    #[snugom(datetime(epoch_millis), created_at, filterable, sortable)]
    pub created_at: DateTime<Utc>,

    /// Entry 63: Validate + filter number
    #[snugom(filterable, validate(range(min = 0, max = 1000)))]
    pub score: u32,

    /// Entry 61: Metadata (not indexed)
    pub metadata: Value,

    /// Entry 62: Created by (not filtered)
    pub created_by: String,
}

/// Entity for testing ascending default sort (Entry 59)
#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1, default_sort = "name")]
pub struct AscendingSortEntity {
    #[snugom(id)]
    pub id: String,

    #[snugom(searchable, sortable)]
    pub name: String,
}

bundle! {
    service: "test",
    entities: {
        NumericEntity => "numeric_items",
        OptionalNumericEntity => "optional_numeric_items",
        BooleanEntity => "boolean_items",
        EnumEntity => "enum_items",
        TextSearchEntity => "text_search_items",
        TagStringEntity => "tag_string_items",
        DateTimeEntity => "datetime_items",
        ArrayEntity => "array_items",
        GeoEntity => "geo_items",
        CombinedEntity => "combined_items",
        AscendingSortEntity => "asc_sort_items",
    }
}

// =============================================================================
// UNIT TESTS - Numeric Fields
// =============================================================================

mod numeric_tests {
    use super::*;

    #[test]
    fn test_numeric_filterable_generates_numeric_index() {
        let def = NumericEntity::index_definition("test");
        let count_field = def.schema.iter().find(|f| f.field_name == "count");

        assert!(count_field.is_some(), "count field should be in schema");
        let field = count_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Numeric));
        assert!(!field.sortable, "count should not be sortable");
    }

    #[test]
    fn test_numeric_sortable_generates_sortable_index() {
        let def = NumericEntity::index_definition("test");
        let score_field = def.schema.iter().find(|f| f.field_name == "score");

        assert!(score_field.is_some(), "score field should be in schema");
        let field = score_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Numeric));
        assert!(field.sortable, "score should be sortable");
    }

    #[test]
    fn test_numeric_filterable_sortable_combined() {
        let def = NumericEntity::index_definition("test");
        let level_field = def.schema.iter().find(|f| f.field_name == "level");

        assert!(level_field.is_some(), "level field should be in schema");
        let field = level_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Numeric));
        assert!(field.sortable, "level should be sortable");

        // Also verify it's in allowed sorts
        let sorts = NumericEntity::allowed_sorts();
        assert!(sorts.iter().any(|s| s.name == "level"), "level should be in allowed sorts");
    }

    #[test]
    fn test_numeric_indexed_only_in_schema_but_not_filterable() {
        let def = NumericEntity::index_definition("test");
        let internal_field = def.schema.iter().find(|f| f.field_name == "internal_score");

        assert!(internal_field.is_some(), "internal_score should be in schema");

        // Verify it's NOT in filter mapping by trying to filter on it
        let descriptor = snugom::search::FilterDescriptor {
            field: "internal_score".to_string(),
            operator: snugom::search::FilterOperator::Eq,
            values: vec!["100".to_string()],
        };
        let result = NumericEntity::map_filter(descriptor);
        assert!(result.is_err(), "internal_score should not be filterable");
    }

    #[test]
    fn test_numeric_alias_filter_mapping() {
        // The alias "num" should map to raw_count
        let descriptor = snugom::search::FilterDescriptor {
            field: "num".to_string(),
            operator: snugom::search::FilterOperator::Eq,
            values: vec!["50".to_string()],
        };
        let result = NumericEntity::map_filter(descriptor);
        assert!(result.is_ok(), "alias 'num' should be filterable");
    }

    #[test]
    fn test_signed_integer_generates_numeric() {
        let def = NumericEntity::index_definition("test");
        let signed_field = def.schema.iter().find(|f| f.field_name == "signed_value");

        assert!(signed_field.is_some(), "signed_value should be in schema");
        let field = signed_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Numeric));
    }

    #[test]
    fn test_float_generates_numeric_sortable() {
        let def = NumericEntity::index_definition("test");
        let rating_field = def.schema.iter().find(|f| f.field_name == "rating");

        assert!(rating_field.is_some(), "rating field should be in schema");
        let field = rating_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Numeric));
        assert!(field.sortable, "rating should be sortable");
    }

    #[test]
    fn test_optional_numeric_filterable() {
        let def = OptionalNumericEntity::index_definition("test");
        let level_field = def.schema.iter().find(|f| f.field_name == "level");

        assert!(level_field.is_some(), "level field should be in schema");
        let field = level_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Numeric));
    }

    #[test]
    fn test_optional_numeric_sortable() {
        let def = OptionalNumericEntity::index_definition("test");
        let rank_field = def.schema.iter().find(|f| f.field_name == "rank");

        assert!(rank_field.is_some(), "rank field should be in schema");
        let field = rank_field.unwrap();
        assert!(field.sortable, "rank should be sortable");
    }

    #[test]
    fn test_optional_numeric_alias() {
        let descriptor = snugom::search::FilterDescriptor {
            field: "pts".to_string(),
            operator: snugom::search::FilterOperator::Eq,
            values: vec!["100".to_string()],
        };
        let result = OptionalNumericEntity::map_filter(descriptor);
        assert!(result.is_ok(), "alias 'pts' should be filterable");
    }
}

// =============================================================================
// UNIT TESTS - Boolean Fields
// =============================================================================

mod boolean_tests {
    use super::*;

    #[test]
    fn test_bool_filterable_generates_tag_index() {
        let def = BooleanEntity::index_definition("test");
        let active_field = def.schema.iter().find(|f| f.field_name == "active");

        assert!(active_field.is_some(), "active field should be in schema");
        let field = active_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Tag));
    }

    #[test]
    fn test_bool_alias_mapping() {
        let descriptor = snugom::search::FilterDescriptor {
            field: "is_enabled".to_string(),
            operator: snugom::search::FilterOperator::Eq,
            values: vec!["true".to_string()],
        };
        let result = BooleanEntity::map_filter(descriptor);
        assert!(result.is_ok(), "alias 'is_enabled' should be filterable");
    }

    #[test]
    fn test_bool_indexed_only_not_filterable() {
        let def = BooleanEntity::index_definition("test");
        let internal_field = def.schema.iter().find(|f| f.field_name == "internal_flag");

        assert!(internal_field.is_some(), "internal_flag should be in schema");

        let descriptor = snugom::search::FilterDescriptor {
            field: "internal_flag".to_string(),
            operator: snugom::search::FilterOperator::Eq,
            values: vec!["true".to_string()],
        };
        let result = BooleanEntity::map_filter(descriptor);
        assert!(result.is_err(), "internal_flag should not be filterable");
    }

    #[test]
    fn test_optional_bool_filterable() {
        let def = BooleanEntity::index_definition("test");
        let verified_field = def.schema.iter().find(|f| f.field_name == "verified");

        assert!(verified_field.is_some(), "verified field should be in schema");
        let field = verified_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Tag));
    }
}

// =============================================================================
// UNIT TESTS - Enum Fields
// =============================================================================

mod enum_tests {
    use super::*;

    #[test]
    fn test_enum_filterable_generates_tag_index() {
        let def = EnumEntity::index_definition("test");
        // Enum fields with filterable are indexed with __<field>_tag naming convention
        let status_field = def.schema.iter().find(|f| f.field_name == "__status_tag");

        assert!(status_field.is_some(), "status field should be in schema");
        let field = status_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Tag));
    }

    #[test]
    fn test_enum_filter_mapping() {
        let descriptor = snugom::search::FilterDescriptor {
            field: "status".to_string(),
            operator: snugom::search::FilterOperator::Eq,
            values: vec!["active".to_string()],
        };
        let result = EnumEntity::map_filter(descriptor);
        assert!(result.is_ok(), "status should be filterable");
    }

    #[test]
    fn test_enum_alias_mapping() {
        let descriptor = snugom::search::FilterDescriptor {
            field: "state".to_string(),
            operator: snugom::search::FilterOperator::Eq,
            values: vec!["pending".to_string()],
        };
        let result = EnumEntity::map_filter(descriptor);
        assert!(result.is_ok(), "alias 'state' should be filterable");
    }

    #[test]
    fn test_enum_indexed_only_not_filterable() {
        let def = EnumEntity::index_definition("test");
        let internal_field = def.schema.iter().find(|f| f.field_name == "internal_status");

        assert!(internal_field.is_some(), "internal_status should be in schema");

        let descriptor = snugom::search::FilterDescriptor {
            field: "internal_status".to_string(),
            operator: snugom::search::FilterOperator::Eq,
            values: vec!["active".to_string()],
        };
        let result = EnumEntity::map_filter(descriptor);
        assert!(result.is_err(), "internal_status should not be filterable");
    }

    #[test]
    fn test_optional_enum_filterable() {
        let def = EnumEntity::index_definition("test");
        // Enum fields with filterable are indexed with __<field>_tag naming convention
        let role_field = def.schema.iter().find(|f| f.field_name == "__role_tag");

        assert!(role_field.is_some(), "role field should be in schema");
        let field = role_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Tag));
    }

    #[test]
    fn test_enum_sortable_generates_tag_sortable() {
        let def = EnumEntity::index_definition("test");
        // Enum fields with filterable are indexed with __<field>_tag naming convention
        let priority_field = def.schema.iter().find(|f| f.field_name == "__priority_tag");

        assert!(priority_field.is_some(), "priority field should be in schema");
        let field = priority_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Tag));
        assert!(field.sortable, "priority should be sortable");
    }
}

// =============================================================================
// UNIT TESTS - String TEXT Fields
// =============================================================================

mod text_string_tests {
    use super::*;

    #[test]
    fn test_searchable_generates_text_index() {
        let def = TextSearchEntity::index_definition("test");
        let name_field = def.schema.iter().find(|f| f.field_name == "name");

        assert!(name_field.is_some(), "name field should be in schema");
        let field = name_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Text));
    }

    #[test]
    fn test_searchable_adds_to_text_search_fields() {
        let text_fields = TextSearchEntity::text_search_fields();
        assert!(text_fields.contains(&"name"), "name should be in text_search_fields");
        assert!(text_fields.contains(&"title"), "title should be in text_search_fields");
        assert!(text_fields.contains(&"bio"), "bio should be in text_search_fields");
    }

    #[test]
    fn test_searchable_sortable_combined() {
        let def = TextSearchEntity::index_definition("test");
        let title_field = def.schema.iter().find(|f| f.field_name == "title");

        assert!(title_field.is_some(), "title field should be in schema");
        let field = title_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Text));
        assert!(field.sortable, "title should be sortable");

        let sorts = TextSearchEntity::allowed_sorts();
        assert!(sorts.iter().any(|s| s.name == "title"), "title should be in allowed sorts");
    }

    #[test]
    fn test_optional_searchable() {
        let def = TextSearchEntity::index_definition("test");
        let bio_field = def.schema.iter().find(|f| f.field_name == "bio");

        assert!(bio_field.is_some(), "bio field should be in schema");
        let field = bio_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Text));
    }

    #[test]
    fn test_indexed_text_only_not_in_text_search_fields() {
        let def = TextSearchEntity::index_definition("test");
        let internal_field = def.schema.iter().find(|f| f.field_name == "internal_text");

        assert!(internal_field.is_some(), "internal_text should be in schema");
        let field = internal_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Text));

        // But it should NOT be in text_search_fields (not searchable, just indexed)
        let text_fields = TextSearchEntity::text_search_fields();
        assert!(!text_fields.contains(&"internal_text"), "internal_text should NOT be in text_search_fields");
    }
}

// =============================================================================
// UNIT TESTS - String TAG Fields
// =============================================================================

mod tag_string_tests {
    use super::*;

    #[test]
    fn test_filterable_tag_generates_tag_index() {
        let def = TagStringEntity::index_definition("test");
        let slug_field = def.schema.iter().find(|f| f.field_name == "slug");

        assert!(slug_field.is_some(), "slug field should be in schema");
        let field = slug_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Tag));
    }

    #[test]
    fn test_filterable_tag_filter_mapping() {
        let descriptor = snugom::search::FilterDescriptor {
            field: "slug".to_string(),
            operator: snugom::search::FilterOperator::Eq,
            values: vec!["my-slug".to_string()],
        };
        let result = TagStringEntity::map_filter(descriptor);
        assert!(result.is_ok(), "slug should be filterable");
    }

    #[test]
    fn test_filterable_tag_sortable_combined() {
        let def = TagStringEntity::index_definition("test");
        let region_field = def.schema.iter().find(|f| f.field_name == "region");

        assert!(region_field.is_some(), "region field should be in schema");
        let field = region_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Tag));
        assert!(field.sortable, "region should be sortable");
    }

    #[test]
    fn test_filterable_tag_alias() {
        let descriptor = snugom::search::FilterDescriptor {
            field: "ref".to_string(),
            operator: snugom::search::FilterOperator::Eq,
            values: vec!["abc123".to_string()],
        };
        let result = TagStringEntity::map_filter(descriptor);
        assert!(result.is_ok(), "alias 'ref' should be filterable");
    }

    #[test]
    fn test_indexed_tag_only_not_filterable() {
        let def = TagStringEntity::index_definition("test");
        let internal_field = def.schema.iter().find(|f| f.field_name == "internal_key");

        assert!(internal_field.is_some(), "internal_key should be in schema");

        let descriptor = snugom::search::FilterDescriptor {
            field: "internal_key".to_string(),
            operator: snugom::search::FilterOperator::Eq,
            values: vec!["key".to_string()],
        };
        let result = TagStringEntity::map_filter(descriptor);
        assert!(result.is_err(), "internal_key should not be filterable");
    }

    #[test]
    fn test_optional_tag_filterable() {
        let def = TagStringEntity::index_definition("test");
        let external_id_field = def.schema.iter().find(|f| f.field_name == "external_id");

        assert!(external_id_field.is_some(), "external_id field should be in schema");
        let field = external_id_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Tag));
    }
}

// =============================================================================
// UNIT TESTS - DateTime Fields
// =============================================================================

mod datetime_tests {
    use super::*;

    #[test]
    fn test_datetime_uses_ts_mirror_for_index() {
        let def = DateTimeEntity::index_definition("test");

        // DateTime fields should use _ts suffix
        let expires_field = def.schema.iter().find(|f| f.field_name == "expires_at_ts");
        assert!(expires_field.is_some(), "expires_at_ts should be in schema");
    }

    #[test]
    fn test_datetime_filterable_generates_numeric() {
        let def = DateTimeEntity::index_definition("test");
        let expires_field = def.schema.iter().find(|f| f.field_name == "expires_at_ts");

        assert!(expires_field.is_some(), "expires_at_ts should be in schema");
        let field = expires_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Numeric));
    }

    #[test]
    fn test_datetime_sortable_generates_numeric_sortable() {
        let def = DateTimeEntity::index_definition("test");
        let scheduled_field = def.schema.iter().find(|f| f.field_name == "scheduled_at_ts");

        assert!(scheduled_field.is_some(), "scheduled_at_ts should be in schema");
        let field = scheduled_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Numeric));
        assert!(field.sortable, "scheduled_at_ts should be sortable");
    }

    #[test]
    fn test_datetime_sort_uses_ts_path() {
        let sorts = DateTimeEntity::allowed_sorts();
        let created_sort = sorts.iter().find(|s| s.name == "created_at");

        assert!(created_sort.is_some(), "created_at should be in allowed sorts");
        let sort = created_sort.unwrap();
        assert_eq!(sort.path, "created_at_ts", "sort path should use _ts suffix");
    }

    #[test]
    fn test_datetime_filter_uses_ts_field() {
        // Filter on "expires_at" should query "expires_at_ts"
        let descriptor = snugom::search::FilterDescriptor {
            field: "expires_at".to_string(),
            operator: snugom::search::FilterOperator::Range,
            values: vec!["1704067200000".to_string(), "".to_string()],
        };
        let result = DateTimeEntity::map_filter(descriptor);
        assert!(result.is_ok(), "expires_at should be filterable");
    }

    #[test]
    fn test_datetime_alias_mapping() {
        // "date" alias should map to event_date_ts
        let descriptor = snugom::search::FilterDescriptor {
            field: "date".to_string(),
            operator: snugom::search::FilterOperator::Range,
            values: vec!["1704067200000".to_string(), "".to_string()],
        };
        let result = DateTimeEntity::map_filter(descriptor);
        assert!(result.is_ok(), "alias 'date' should be filterable");
    }

    #[test]
    fn test_optional_datetime_filterable() {
        let def = DateTimeEntity::index_definition("test");
        let published_field = def.schema.iter().find(|f| f.field_name == "published_at_ts");

        assert!(published_field.is_some(), "published_at_ts should be in schema");
    }

    #[test]
    fn test_optional_datetime_sortable() {
        let def = DateTimeEntity::index_definition("test");
        let archived_field = def.schema.iter().find(|f| f.field_name == "archived_at_ts");

        assert!(archived_field.is_some(), "archived_at_ts should be in schema");
        let field = archived_field.unwrap();
        assert!(field.sortable, "archived_at_ts should be sortable");
    }

    #[test]
    fn test_datetime_indexed_only_not_filterable() {
        let def = DateTimeEntity::index_definition("test");
        let internal_field = def.schema.iter().find(|f| f.field_name == "internal_ts_ts");

        assert!(internal_field.is_some(), "internal_ts_ts should be in schema");

        let descriptor = snugom::search::FilterDescriptor {
            field: "internal_ts".to_string(),
            operator: snugom::search::FilterOperator::Eq,
            values: vec!["1704067200000".to_string()],
        };
        let result = DateTimeEntity::map_filter(descriptor);
        assert!(result.is_err(), "internal_ts should not be filterable");
    }

    #[test]
    fn test_default_sort_descending() {
        let default = DateTimeEntity::default_sort();
        assert_eq!(default.name, "created_at");
        assert_eq!(default.default_order, SortOrder::Desc);
    }
}

// =============================================================================
// UNIT TESTS - Array Fields
// =============================================================================

mod array_tests {
    use super::*;

    #[test]
    fn test_vec_string_filterable_generates_tag() {
        let def = ArrayEntity::index_definition("test");
        let tags_field = def.schema.iter().find(|f| f.field_name == "tags");

        assert!(tags_field.is_some(), "tags field should be in schema");
        let field = tags_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Tag));
    }

    #[test]
    fn test_vec_string_filter_mapping() {
        let descriptor = snugom::search::FilterDescriptor {
            field: "tags".to_string(),
            operator: snugom::search::FilterOperator::Eq,
            values: vec!["gaming".to_string()],
        };
        let result = ArrayEntity::map_filter(descriptor);
        assert!(result.is_ok(), "tags should be filterable");
    }

    #[test]
    fn test_vec_string_alias() {
        let descriptor = snugom::search::FilterDescriptor {
            field: "labels".to_string(),
            operator: snugom::search::FilterOperator::Eq,
            values: vec!["featured".to_string()],
        };
        let result = ArrayEntity::map_filter(descriptor);
        assert!(result.is_ok(), "alias 'labels' should be filterable");
    }

    #[test]
    fn test_vec_string_indexed_only_not_filterable() {
        let def = ArrayEntity::index_definition("test");
        let internal_field = def.schema.iter().find(|f| f.field_name == "internal_tags");

        assert!(internal_field.is_some(), "internal_tags should be in schema");

        let descriptor = snugom::search::FilterDescriptor {
            field: "internal_tags".to_string(),
            operator: snugom::search::FilterOperator::Eq,
            values: vec!["tag".to_string()],
        };
        let result = ArrayEntity::map_filter(descriptor);
        assert!(result.is_err(), "internal_tags should not be filterable");
    }
}

// =============================================================================
// UNIT TESTS - Geographic Fields
// =============================================================================

mod geo_tests {
    use super::*;

    #[test]
    fn test_geo_filterable_generates_geo_index() {
        let def = GeoEntity::index_definition("test");
        let location_field = def.schema.iter().find(|f| f.field_name == "location");

        assert!(location_field.is_some(), "location field should be in schema");
        let field = location_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Geo));
    }

    #[test]
    fn test_geo_indexed_only() {
        let def = GeoEntity::index_definition("test");
        let internal_field = def.schema.iter().find(|f| f.field_name == "internal_geo");

        assert!(internal_field.is_some(), "internal_geo should be in schema");
        let field = internal_field.unwrap();
        assert!(matches!(field.field_type, IndexFieldType::Geo));
    }
}

// =============================================================================
// UNIT TESTS - Combined/Complex Scenarios
// =============================================================================

mod combined_tests {
    use super::*;

    #[test]
    fn test_multiple_searchable_fields_in_text_search() {
        let text_fields = CombinedEntity::text_search_fields();
        assert!(text_fields.contains(&"name"), "name should be in text_search_fields");
        assert!(text_fields.contains(&"description"), "description should be in text_search_fields");
        assert_eq!(text_fields.len(), 2, "should have exactly 2 text search fields");
    }

    #[test]
    fn test_default_sort_descending_prefix() {
        let default = CombinedEntity::default_sort();
        assert_eq!(default.name, "created_at");
        assert_eq!(default.default_order, SortOrder::Desc);
    }

    #[test]
    fn test_default_sort_ascending_no_prefix() {
        let default = AscendingSortEntity::default_sort();
        assert_eq!(default.name, "name");
        assert_eq!(default.default_order, SortOrder::Asc);
    }

    #[test]
    fn test_unattributed_fields_not_indexed() {
        let def = CombinedEntity::index_definition("test");

        // metadata should NOT be in schema
        let metadata_field = def.schema.iter().find(|f| f.field_name == "metadata");
        assert!(metadata_field.is_none(), "metadata should NOT be in schema");

        // created_by should NOT be in schema
        let created_by_field = def.schema.iter().find(|f| f.field_name == "created_by");
        assert!(created_by_field.is_none(), "created_by should NOT be in schema");
    }

    #[test]
    fn test_id_field_not_indexed() {
        let def = CombinedEntity::index_definition("test");

        // id field should NOT be in the index schema
        let id_field = def.schema.iter().find(|f| f.field_name == "id");
        assert!(id_field.is_none(), "id should NOT be in schema");
    }

    #[test]
    fn test_index_name_includes_collection() {
        let def = CombinedEntity::index_definition("myprefix");
        assert_eq!(def.name, "myprefix:test:combined_items:idx");
    }

    #[test]
    fn test_index_prefix_format() {
        let def = CombinedEntity::index_definition("myprefix");
        assert_eq!(def.prefixes[0], "myprefix:test:combined_items:");
    }

    #[test]
    fn test_validation_and_filter_combined() {
        // The score field has both validate and filterable
        let def = CombinedEntity::index_definition("test");
        let score_field = def.schema.iter().find(|f| f.field_name == "score");

        assert!(score_field.is_some(), "score field should be in schema");

        // And it should be filterable
        let descriptor = snugom::search::FilterDescriptor {
            field: "score".to_string(),
            operator: snugom::search::FilterOperator::Range,
            values: vec!["0".to_string(), "100".to_string()],
        };
        let result = CombinedEntity::map_filter(descriptor);
        assert!(result.is_ok(), "score should be filterable");
    }
}

// =============================================================================
// UNIT TESTS - Unknown Filter Field Error
// =============================================================================

mod error_tests {
    use super::*;

    #[test]
    fn test_unknown_filter_field_returns_error() {
        let descriptor = snugom::search::FilterDescriptor {
            field: "nonexistent_field".to_string(),
            operator: snugom::search::FilterOperator::Eq,
            values: vec!["value".to_string()],
        };
        let result = NumericEntity::map_filter(descriptor);
        assert!(result.is_err(), "unknown field should return error");

        let err = result.unwrap_err();
        match err {
            snugom::errors::RepoError::InvalidRequest { message } => {
                assert!(message.contains("Unknown filter field"), "error should mention unknown field");
            }
            _ => panic!("expected InvalidRequest error"),
        }
    }
}

// =============================================================================
// INTEGRATION TESTS - Require Redis
// =============================================================================

#[cfg(test)]
mod integration_tests {
    use super::*;
    use redis::aio::ConnectionManager;
    use serial_test::serial;
    use snugom::repository::Repo;
    use snugom::search::{SearchEntity, SearchQuery};

    async fn get_redis_connection() -> ConnectionManager {
        let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
        let client = redis::Client::open(redis_url).expect("Failed to create Redis client");
        ConnectionManager::new(client).await.expect("Failed to connect to Redis")
    }

    async fn cleanup_keys(conn: &mut ConnectionManager, pattern: &str) {
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(pattern)
            .query_async(conn)
            .await
            .unwrap_or_default();
        if !keys.is_empty() {
            let _: () = redis::cmd("DEL")
                .arg(&keys)
                .query_async(conn)
                .await
                .unwrap_or(());
        }
    }

    async fn drop_index_if_exists(conn: &mut ConnectionManager, index_name: &str) {
        let _: Result<(), redis::RedisError> = redis::cmd("FT.DROPINDEX")
            .arg(index_name)
            .query_async(conn)
            .await;
    }

    /// Simple entity for integration testing
    #[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
    #[snugom(version = 1, default_sort = "-score")]
    pub struct IntegrationTestEntity {
        #[snugom(id)]
        pub id: String,

        #[snugom(searchable, sortable)]
        pub name: String,

        #[snugom(filterable, sortable)]
        pub score: u32,

        #[snugom(filterable(tag))]
        pub category: String,

        #[snugom(filterable)]
        pub active: bool,

        #[snugom(datetime(epoch_millis), created_at, filterable, sortable)]
        pub created_at: DateTime<Utc>,
    }

    #[tokio::test]
    #[serial]
    async fn test_integration_search_with_numeric_filter() {
        let mut conn = get_redis_connection().await;
        let prefix = "search_test";

        // Cleanup
        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;

        let repo: Repo<IntegrationTestEntity> = Repo::new(prefix.to_string());

        // Ensure index
        repo.ensure_search_index(&mut conn).await.expect("Failed to create index");

        // Create test entities
        let now = chrono::Utc::now();
        for i in 1..=5 {
            let entity = IntegrationTestEntity::validation_builder()
                .id(format!("item-{i}"))
                .name(format!("Item {i}"))
                .score(i * 10)
                .category("test".to_string())
                .active(true)
                .created_at(now)
                .build()
                .expect("valid entity");

            snugom::run! {
                &repo,
                &mut conn,
                create => IntegrationTestEntity {
                    id: entity.id,
                    name: entity.name,
                    score: entity.score,
                    category: entity.category,
                    active: entity.active,
                    created_at: entity.created_at,
                }
            }
            .expect("create should succeed");
        }

        // Give Redis time to index
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Search with numeric range filter
        let query = SearchQuery {
            page: Some(1),
            page_size: Some(10),
            sort_by: None,
            sort_order: None,
            q: None,
            filter: vec!["score:range:20,40".to_string()],
        };

        let params = query
            .with_text_query(
                IntegrationTestEntity::allowed_sorts(),
                IntegrationTestEntity::default_sort(),
                |d| IntegrationTestEntity::map_filter(d),
                IntegrationTestEntity::text_search_fields(),
            )
            .expect("valid params");

        let result = repo.search(&mut conn, params).await.expect("search should succeed");

        // Should find items with score 20, 30, 40 (3 items)
        assert_eq!(result.items.len(), 3, "should find 3 items with score 20-40");

        // Cleanup
        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;
    }

    #[tokio::test]
    #[serial]
    async fn test_integration_search_with_tag_filter() {
        let mut conn = get_redis_connection().await;
        let prefix = "search_test2";

        // Cleanup
        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;

        let repo: Repo<IntegrationTestEntity> = Repo::new(prefix.to_string());
        repo.ensure_search_index(&mut conn).await.expect("Failed to create index");

        let now = chrono::Utc::now();

        // Create entities with different categories
        for (i, cat) in [(1, "alpha"), (2, "beta"), (3, "alpha"), (4, "gamma"), (5, "alpha")].iter() {
            let entity = IntegrationTestEntity::validation_builder()
                .id(format!("item-{i}"))
                .name(format!("Item {i}"))
                .score(*i * 10)
                .category(cat.to_string())
                .active(true)
                .created_at(now)
                .build()
                .expect("valid entity");

            snugom::run! {
                &repo,
                &mut conn,
                create => IntegrationTestEntity {
                    id: entity.id,
                    name: entity.name,
                    score: entity.score,
                    category: entity.category,
                    active: entity.active,
                    created_at: entity.created_at,
                }
            }
            .expect("create should succeed");
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Search with tag filter
        let query = SearchQuery {
            page: Some(1),
            page_size: Some(10),
            sort_by: None,
            sort_order: None,
            q: None,
            filter: vec!["category:eq:alpha".to_string()],
        };

        let params = query
            .with_text_query(
                IntegrationTestEntity::allowed_sorts(),
                IntegrationTestEntity::default_sort(),
                |d| IntegrationTestEntity::map_filter(d),
                IntegrationTestEntity::text_search_fields(),
            )
            .expect("valid params");

        let result = repo.search(&mut conn, params).await.expect("search should succeed");

        assert_eq!(result.items.len(), 3, "should find 3 items with category=alpha");

        // Cleanup
        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;
    }

    #[tokio::test]
    #[serial]
    async fn test_integration_search_with_text_query() {
        let mut conn = get_redis_connection().await;
        let prefix = "search_test3";

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;

        let repo: Repo<IntegrationTestEntity> = Repo::new(prefix.to_string());
        repo.ensure_search_index(&mut conn).await.expect("Failed to create index");

        let now = chrono::Utc::now();

        // Create entities with different names
        for (i, name) in [(1, "Dragon Slayer"), (2, "Knight Commander"), (3, "Dragon Knight"), (4, "Mage")].iter() {
            let entity = IntegrationTestEntity::validation_builder()
                .id(format!("item-{i}"))
                .name(name.to_string())
                .score(*i * 10)
                .category("hero".to_string())
                .active(true)
                .created_at(now)
                .build()
                .expect("valid entity");

            snugom::run! {
                &repo,
                &mut conn,
                create => IntegrationTestEntity {
                    id: entity.id,
                    name: entity.name,
                    score: entity.score,
                    category: entity.category,
                    active: entity.active,
                    created_at: entity.created_at,
                }
            }
            .expect("create should succeed");
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Search with full-text query
        let query = SearchQuery {
            page: Some(1),
            page_size: Some(10),
            sort_by: None,
            sort_order: None,
            q: Some("dragon".to_string()),
            filter: vec![],
        };

        let params = query
            .with_text_query(
                IntegrationTestEntity::allowed_sorts(),
                IntegrationTestEntity::default_sort(),
                |d| IntegrationTestEntity::map_filter(d),
                IntegrationTestEntity::text_search_fields(),
            )
            .expect("valid params");

        let result = repo.search(&mut conn, params).await.expect("search should succeed");

        assert_eq!(result.items.len(), 2, "should find 2 items with 'dragon' in name");

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;
    }

    #[tokio::test]
    #[serial]
    async fn test_integration_search_with_sorting() {
        let mut conn = get_redis_connection().await;
        let prefix = "search_test4";

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;

        let repo: Repo<IntegrationTestEntity> = Repo::new(prefix.to_string());
        repo.ensure_search_index(&mut conn).await.expect("Failed to create index");

        let now = chrono::Utc::now();

        // Create entities with different scores
        for i in [3, 1, 5, 2, 4].iter() {
            let entity = IntegrationTestEntity::validation_builder()
                .id(format!("item-{i}"))
                .name(format!("Item {i}"))
                .score(*i * 10)
                .category("test".to_string())
                .active(true)
                .created_at(now)
                .build()
                .expect("valid entity");

            snugom::run! {
                &repo,
                &mut conn,
                create => IntegrationTestEntity {
                    id: entity.id,
                    name: entity.name,
                    score: entity.score,
                    category: entity.category,
                    active: entity.active,
                    created_at: entity.created_at,
                }
            }
            .expect("create should succeed");
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Search with ascending sort
        let query = SearchQuery {
            page: Some(1),
            page_size: Some(10),
            sort_by: Some("score".to_string()),
            sort_order: Some(SortOrder::Asc),
            q: None,
            filter: vec![],
        };

        let params = query
            .with_text_query(
                IntegrationTestEntity::allowed_sorts(),
                IntegrationTestEntity::default_sort(),
                |d| IntegrationTestEntity::map_filter(d),
                IntegrationTestEntity::text_search_fields(),
            )
            .expect("valid params");

        let result = repo.search(&mut conn, params).await.expect("search should succeed");

        assert_eq!(result.items.len(), 5, "should find all 5 items");

        // Verify ascending order
        let scores: Vec<u32> = result.items.iter().map(|i| i.score).collect();
        assert_eq!(scores, vec![10, 20, 30, 40, 50], "should be sorted ascending");

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;
    }

    #[tokio::test]
    #[serial]
    async fn test_integration_search_with_boolean_filter() {
        let mut conn = get_redis_connection().await;
        let prefix = "search_test5";

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;

        let repo: Repo<IntegrationTestEntity> = Repo::new(prefix.to_string());
        repo.ensure_search_index(&mut conn).await.expect("Failed to create index");

        let now = chrono::Utc::now();

        // Create entities with different active states
        for (i, active) in [(1, true), (2, false), (3, true), (4, false), (5, true)].iter() {
            let entity = IntegrationTestEntity::validation_builder()
                .id(format!("item-{i}"))
                .name(format!("Item {i}"))
                .score(*i * 10)
                .category("test".to_string())
                .active(*active)
                .created_at(now)
                .build()
                .expect("valid entity");

            snugom::run! {
                &repo,
                &mut conn,
                create => IntegrationTestEntity {
                    id: entity.id,
                    name: entity.name,
                    score: entity.score,
                    category: entity.category,
                    active: entity.active,
                    created_at: entity.created_at,
                }
            }
            .expect("create should succeed");
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Search for active items
        let query = SearchQuery {
            page: Some(1),
            page_size: Some(10),
            sort_by: None,
            sort_order: None,
            q: None,
            filter: vec!["active:eq:true".to_string()],
        };

        let params = query
            .with_text_query(
                IntegrationTestEntity::allowed_sorts(),
                IntegrationTestEntity::default_sort(),
                |d| IntegrationTestEntity::map_filter(d),
                IntegrationTestEntity::text_search_fields(),
            )
            .expect("valid params");

        let result = repo.search(&mut conn, params).await.expect("search should succeed");

        assert_eq!(result.items.len(), 3, "should find 3 active items");

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;
    }

    #[tokio::test]
    #[serial]
    async fn test_integration_search_with_combined_filters() {
        let mut conn = get_redis_connection().await;
        let prefix = "search_test6";

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;

        let repo: Repo<IntegrationTestEntity> = Repo::new(prefix.to_string());
        repo.ensure_search_index(&mut conn).await.expect("Failed to create index");

        let now = chrono::Utc::now();

        // Create varied entities
        let test_data = [
            (1, "Alpha", 10, "cat1", true),
            (2, "Beta", 20, "cat1", false),
            (3, "Gamma", 30, "cat2", true),
            (4, "Delta", 40, "cat1", true),
            (5, "Epsilon", 50, "cat2", false),
        ];

        for (i, name, score, cat, active) in test_data.iter() {
            let entity = IntegrationTestEntity::validation_builder()
                .id(format!("item-{i}"))
                .name(name.to_string())
                .score(*score)
                .category(cat.to_string())
                .active(*active)
                .created_at(now)
                .build()
                .expect("valid entity");

            snugom::run! {
                &repo,
                &mut conn,
                create => IntegrationTestEntity {
                    id: entity.id,
                    name: entity.name,
                    score: entity.score,
                    category: entity.category,
                    active: entity.active,
                    created_at: entity.created_at,
                }
            }
            .expect("create should succeed");
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Search with multiple filters: category=cat1 AND active=true
        let query = SearchQuery {
            page: Some(1),
            page_size: Some(10),
            sort_by: Some("score".to_string()),
            sort_order: Some(SortOrder::Asc),
            q: None,
            filter: vec!["category:eq:cat1".to_string(), "active:eq:true".to_string()],
        };

        let params = query
            .with_text_query(
                IntegrationTestEntity::allowed_sorts(),
                IntegrationTestEntity::default_sort(),
                |d| IntegrationTestEntity::map_filter(d),
                IntegrationTestEntity::text_search_fields(),
            )
            .expect("valid params");

        let result = repo.search(&mut conn, params).await.expect("search should succeed");

        // Should find items 1 (Alpha) and 4 (Delta) - both cat1 and active
        assert_eq!(result.items.len(), 2, "should find 2 items matching both filters");

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;
    }

    #[tokio::test]
    #[serial]
    async fn test_integration_search_pagination() {
        let mut conn = get_redis_connection().await;
        let prefix = "search_test7";

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;

        let repo: Repo<IntegrationTestEntity> = Repo::new(prefix.to_string());
        repo.ensure_search_index(&mut conn).await.expect("Failed to create index");

        let now = chrono::Utc::now();

        // Create 10 entities
        for i in 1..=10 {
            let entity = IntegrationTestEntity::validation_builder()
                .id(format!("item-{i:02}"))
                .name(format!("Item {i:02}"))
                .score(i * 10)
                .category("test".to_string())
                .active(true)
                .created_at(now)
                .build()
                .expect("valid entity");

            snugom::run! {
                &repo,
                &mut conn,
                create => IntegrationTestEntity {
                    id: entity.id,
                    name: entity.name,
                    score: entity.score,
                    category: entity.category,
                    active: entity.active,
                    created_at: entity.created_at,
                }
            }
            .expect("create should succeed");
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Get page 1 with size 3
        let query = SearchQuery {
            page: Some(1),
            page_size: Some(3),
            sort_by: Some("score".to_string()),
            sort_order: Some(SortOrder::Asc),
            q: None,
            filter: vec![],
        };

        let params = query
            .with_text_query(
                IntegrationTestEntity::allowed_sorts(),
                IntegrationTestEntity::default_sort(),
                |d| IntegrationTestEntity::map_filter(d),
                IntegrationTestEntity::text_search_fields(),
            )
            .expect("valid params");

        let result = repo.search(&mut conn, params).await.expect("search should succeed");

        assert_eq!(result.items.len(), 3, "page 1 should have 3 items");
        assert_eq!(result.total, 10, "total should be 10");
        assert_eq!(result.items[0].score, 10, "first item should have score 10");

        // Get page 2
        let query2 = SearchQuery {
            page: Some(2),
            page_size: Some(3),
            sort_by: Some("score".to_string()),
            sort_order: Some(SortOrder::Asc),
            q: None,
            filter: vec![],
        };

        let params2 = query2
            .with_text_query(
                IntegrationTestEntity::allowed_sorts(),
                IntegrationTestEntity::default_sort(),
                |d| IntegrationTestEntity::map_filter(d),
                IntegrationTestEntity::text_search_fields(),
            )
            .expect("valid params");

        let result2 = repo.search(&mut conn, params2).await.expect("search should succeed");

        assert_eq!(result2.items.len(), 3, "page 2 should have 3 items");
        assert_eq!(result2.items[0].score, 40, "first item on page 2 should have score 40");

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;
    }

    // =========================================================================
    // INTEGRATION TESTS - FilterCondition::Or
    // =========================================================================

    #[tokio::test]
    #[serial]
    async fn test_integration_filter_condition_or_returns_matching_excludes_non_matching() {
        use snugom::search::{FilterCondition, SearchParams, SearchSort};

        let mut conn = get_redis_connection().await;
        let prefix = "or_test1";

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;

        let repo: Repo<IntegrationTestEntity> = Repo::new(prefix.to_string());
        repo.ensure_search_index(&mut conn).await.expect("Failed to create index");

        let now = chrono::Utc::now();

        // Create test data:
        // - Items 1,2,3 have category "alpha" (should match)
        // - Items 4,5 have category "beta" (should match)
        // - Items 6,7 have category "gamma" (should NOT match)
        let test_data = [
            (1, "alpha"),
            (2, "alpha"),
            (3, "alpha"),
            (4, "beta"),
            (5, "beta"),
            (6, "gamma"),
            (7, "gamma"),
        ];

        for (i, cat) in test_data.iter() {
            let entity = IntegrationTestEntity::validation_builder()
                .id(format!("item-{i}"))
                .name(format!("Item {i}"))
                .score(*i * 10)
                .category(cat.to_string())
                .active(true)
                .created_at(now)
                .build()
                .expect("valid entity");

            snugom::run! {
                &repo,
                &mut conn,
                create => IntegrationTestEntity {
                    id: entity.id,
                    name: entity.name,
                    score: entity.score,
                    category: entity.category,
                    active: entity.active,
                    created_at: entity.created_at,
                }
            }
            .expect("create should succeed");
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Search with OR: category=alpha OR category=beta
        let or_condition = FilterCondition::or([
            FilterCondition::tag_eq("category", "alpha"),
            FilterCondition::tag_eq("category", "beta"),
        ]);

        let params = SearchParams::new()
            .with_condition(or_condition)
            .with_sort(Some(SearchSort {
                field: "score".to_string(),
                order: SortOrder::Asc,
            }));

        let result = repo.search(&mut conn, params).await.expect("search should succeed");

        // Should find 5 items (3 alpha + 2 beta), NOT the 2 gamma items
        assert_eq!(result.total, 5, "should find exactly 5 items matching alpha OR beta");
        assert_eq!(result.items.len(), 5);

        // Verify the correct items were returned
        let categories: Vec<&str> = result.items.iter().map(|i| i.category.as_str()).collect();
        assert!(categories.iter().all(|c| *c == "alpha" || *c == "beta"),
            "all returned items should be alpha or beta, got: {:?}", categories);

        // Verify gamma items were NOT returned
        let ids: Vec<&str> = result.items.iter().map(|i| i.id.as_str()).collect();
        assert!(!ids.contains(&"item-6"), "gamma item-6 should NOT be returned");
        assert!(!ids.contains(&"item-7"), "gamma item-7 should NOT be returned");

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;
    }

    #[tokio::test]
    #[serial]
    async fn test_integration_filter_condition_or_with_boolean_and_tag() {
        use snugom::search::{FilterCondition, SearchParams, SearchSort};

        let mut conn = get_redis_connection().await;
        let prefix = "or_test2";

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;

        let repo: Repo<IntegrationTestEntity> = Repo::new(prefix.to_string());
        repo.ensure_search_index(&mut conn).await.expect("Failed to create index");

        let now = chrono::Utc::now();

        // Create test data:
        // - Item 1: active=true, category=other (matches because active)
        // - Item 2: active=false, category=special (matches because category)
        // - Item 3: active=true, category=special (matches both)
        // - Item 4: active=false, category=other (matches NEITHER - should NOT be returned)
        let test_data = [
            (1, true, "other"),
            (2, false, "special"),
            (3, true, "special"),
            (4, false, "other"),
        ];

        for (i, active, cat) in test_data.iter() {
            let entity = IntegrationTestEntity::validation_builder()
                .id(format!("item-{i}"))
                .name(format!("Item {i}"))
                .score(*i * 10)
                .category(cat.to_string())
                .active(*active)
                .created_at(now)
                .build()
                .expect("valid entity");

            snugom::run! {
                &repo,
                &mut conn,
                create => IntegrationTestEntity {
                    id: entity.id,
                    name: entity.name,
                    score: entity.score,
                    category: entity.category,
                    active: entity.active,
                    created_at: entity.created_at,
                }
            }
            .expect("create should succeed");
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Search with OR: active=true OR category=special
        let or_condition = FilterCondition::or([
            FilterCondition::bool_eq("active", true),
            FilterCondition::tag_eq("category", "special"),
        ]);

        let params = SearchParams::new()
            .with_condition(or_condition)
            .with_sort(Some(SearchSort {
                field: "score".to_string(),
                order: SortOrder::Asc,
            }));

        let result = repo.search(&mut conn, params).await.expect("search should succeed");

        // Should find items 1, 2, 3 but NOT item 4
        assert_eq!(result.total, 3, "should find exactly 3 items");

        let ids: Vec<&str> = result.items.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"item-1"), "item-1 (active=true) should be returned");
        assert!(ids.contains(&"item-2"), "item-2 (category=special) should be returned");
        assert!(ids.contains(&"item-3"), "item-3 (both) should be returned");
        assert!(!ids.contains(&"item-4"), "item-4 (neither) should NOT be returned");

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;
    }

    // =========================================================================
    // INTEGRATION TESTS - FilterCondition::And
    // =========================================================================

    #[tokio::test]
    #[serial]
    async fn test_integration_filter_condition_and_returns_only_both_matching() {
        use snugom::search::{FilterCondition, SearchParams, SearchSort};

        let mut conn = get_redis_connection().await;
        let prefix = "and_test1";

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;

        let repo: Repo<IntegrationTestEntity> = Repo::new(prefix.to_string());
        repo.ensure_search_index(&mut conn).await.expect("Failed to create index");

        let now = chrono::Utc::now();

        // Create test data:
        // - Item 1: active=true, category=target (matches BOTH - should be returned)
        // - Item 2: active=true, category=target (matches BOTH - should be returned)
        // - Item 3: active=false, category=target (matches only category - NOT returned)
        // - Item 4: active=true, category=other (matches only active - NOT returned)
        // - Item 5: active=false, category=other (matches NEITHER - NOT returned)
        let test_data = [
            (1, true, "target"),
            (2, true, "target"),
            (3, false, "target"),
            (4, true, "other"),
            (5, false, "other"),
        ];

        for (i, active, cat) in test_data.iter() {
            let entity = IntegrationTestEntity::validation_builder()
                .id(format!("item-{i}"))
                .name(format!("Item {i}"))
                .score(*i * 10)
                .category(cat.to_string())
                .active(*active)
                .created_at(now)
                .build()
                .expect("valid entity");

            snugom::run! {
                &repo,
                &mut conn,
                create => IntegrationTestEntity {
                    id: entity.id,
                    name: entity.name,
                    score: entity.score,
                    category: entity.category,
                    active: entity.active,
                    created_at: entity.created_at,
                }
            }
            .expect("create should succeed");
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Search with AND: active=true AND category=target
        let and_condition = FilterCondition::and([
            FilterCondition::bool_eq("active", true),
            FilterCondition::tag_eq("category", "target"),
        ]);

        let params = SearchParams::new()
            .with_condition(and_condition)
            .with_sort(Some(SearchSort {
                field: "score".to_string(),
                order: SortOrder::Asc,
            }));

        let result = repo.search(&mut conn, params).await.expect("search should succeed");

        // Should find ONLY items 1 and 2
        assert_eq!(result.total, 2, "should find exactly 2 items matching BOTH conditions");

        let ids: Vec<&str> = result.items.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"item-1"), "item-1 should be returned");
        assert!(ids.contains(&"item-2"), "item-2 should be returned");
        assert!(!ids.contains(&"item-3"), "item-3 (only category) should NOT be returned");
        assert!(!ids.contains(&"item-4"), "item-4 (only active) should NOT be returned");
        assert!(!ids.contains(&"item-5"), "item-5 (neither) should NOT be returned");

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;
    }

    #[tokio::test]
    #[serial]
    async fn test_integration_filter_condition_and_with_numeric_range() {
        use snugom::search::{FilterCondition, SearchParams, SearchSort};

        let mut conn = get_redis_connection().await;
        let prefix = "and_test2";

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;

        let repo: Repo<IntegrationTestEntity> = Repo::new(prefix.to_string());
        repo.ensure_search_index(&mut conn).await.expect("Failed to create index");

        let now = chrono::Utc::now();

        // Create test data with various scores and categories:
        // We want: category=premium AND score between 30-60
        let test_data = [
            (1, 20, "premium"),  // score too low
            (2, 30, "premium"),  // matches!
            (3, 50, "premium"),  // matches!
            (4, 60, "premium"),  // matches!
            (5, 70, "premium"),  // score too high
            (6, 40, "standard"), // wrong category
            (7, 50, "standard"), // wrong category
        ];

        for (i, score, cat) in test_data.iter() {
            let entity = IntegrationTestEntity::validation_builder()
                .id(format!("item-{i}"))
                .name(format!("Item {i}"))
                .score(*score)
                .category(cat.to_string())
                .active(true)
                .created_at(now)
                .build()
                .expect("valid entity");

            snugom::run! {
                &repo,
                &mut conn,
                create => IntegrationTestEntity {
                    id: entity.id,
                    name: entity.name,
                    score: entity.score,
                    category: entity.category,
                    active: entity.active,
                    created_at: entity.created_at,
                }
            }
            .expect("create should succeed");
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Search with AND: category=premium AND score in [30, 60]
        let and_condition = FilterCondition::and([
            FilterCondition::tag_eq("category", "premium"),
            FilterCondition::NumericRange {
                field: "score".to_string(),
                min: Some(30.0),
                max: Some(60.0),
            },
        ]);

        let params = SearchParams::new()
            .with_condition(and_condition)
            .with_sort(Some(SearchSort {
                field: "score".to_string(),
                order: SortOrder::Asc,
            }));

        let result = repo.search(&mut conn, params).await.expect("search should succeed");

        // Should find items 2, 3, 4 only
        assert_eq!(result.total, 3, "should find exactly 3 items");

        let ids: Vec<&str> = result.items.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"item-2"), "item-2 should be returned");
        assert!(ids.contains(&"item-3"), "item-3 should be returned");
        assert!(ids.contains(&"item-4"), "item-4 should be returned");

        // Verify non-matching items NOT returned
        assert!(!ids.contains(&"item-1"), "item-1 (score too low) should NOT be returned");
        assert!(!ids.contains(&"item-5"), "item-5 (score too high) should NOT be returned");
        assert!(!ids.contains(&"item-6"), "item-6 (wrong category) should NOT be returned");
        assert!(!ids.contains(&"item-7"), "item-7 (wrong category) should NOT be returned");

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;
    }

    // =========================================================================
    // INTEGRATION TESTS - Nested And/Or Combinations
    // =========================================================================

    #[tokio::test]
    #[serial]
    async fn test_integration_nested_or_within_and() {
        use snugom::search::{FilterCondition, SearchParams, SearchSort};

        let mut conn = get_redis_connection().await;
        let prefix = "nested_test1";

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;

        let repo: Repo<IntegrationTestEntity> = Repo::new(prefix.to_string());
        repo.ensure_search_index(&mut conn).await.expect("Failed to create index");

        let now = chrono::Utc::now();

        // Query: active=true AND (category=alpha OR category=beta)
        // - Item 1: active=true, category=alpha -> matches
        // - Item 2: active=true, category=beta -> matches
        // - Item 3: active=true, category=gamma -> NOT (wrong category)
        // - Item 4: active=false, category=alpha -> NOT (not active)
        // - Item 5: active=false, category=gamma -> NOT (neither)
        let test_data = [
            (1, true, "alpha"),
            (2, true, "beta"),
            (3, true, "gamma"),
            (4, false, "alpha"),
            (5, false, "gamma"),
        ];

        for (i, active, cat) in test_data.iter() {
            let entity = IntegrationTestEntity::validation_builder()
                .id(format!("item-{i}"))
                .name(format!("Item {i}"))
                .score(*i * 10)
                .category(cat.to_string())
                .active(*active)
                .created_at(now)
                .build()
                .expect("valid entity");

            snugom::run! {
                &repo,
                &mut conn,
                create => IntegrationTestEntity {
                    id: entity.id,
                    name: entity.name,
                    score: entity.score,
                    category: entity.category,
                    active: entity.active,
                    created_at: entity.created_at,
                }
            }
            .expect("create should succeed");
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Nested: active=true AND (category=alpha OR category=beta)
        let condition = FilterCondition::and([
            FilterCondition::bool_eq("active", true),
            FilterCondition::or([
                FilterCondition::tag_eq("category", "alpha"),
                FilterCondition::tag_eq("category", "beta"),
            ]),
        ]);

        let params = SearchParams::new()
            .with_condition(condition)
            .with_sort(Some(SearchSort {
                field: "score".to_string(),
                order: SortOrder::Asc,
            }));

        let result = repo.search(&mut conn, params).await.expect("search should succeed");

        // Should find ONLY items 1 and 2
        assert_eq!(result.total, 2, "should find exactly 2 items");

        let ids: Vec<&str> = result.items.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"item-1"), "item-1 should be returned");
        assert!(ids.contains(&"item-2"), "item-2 should be returned");
        assert!(!ids.contains(&"item-3"), "item-3 (gamma) should NOT be returned");
        assert!(!ids.contains(&"item-4"), "item-4 (not active) should NOT be returned");
        assert!(!ids.contains(&"item-5"), "item-5 should NOT be returned");

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;
    }

    #[tokio::test]
    #[serial]
    async fn test_integration_nested_and_within_or() {
        use snugom::search::{FilterCondition, SearchParams, SearchSort};

        let mut conn = get_redis_connection().await;
        let prefix = "nested_test2";

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;

        let repo: Repo<IntegrationTestEntity> = Repo::new(prefix.to_string());
        repo.ensure_search_index(&mut conn).await.expect("Failed to create index");

        let now = chrono::Utc::now();

        // Query: (active=true AND category=premium) OR (score >= 80)
        // - Item 1: active=true, category=premium, score=10 -> matches (first AND)
        // - Item 2: active=false, category=premium, score=20 -> NOT (not active, score too low)
        // - Item 3: active=true, category=standard, score=30 -> NOT (wrong category, score too low)
        // - Item 4: active=false, category=standard, score=80 -> matches (second condition)
        // - Item 5: active=false, category=other, score=90 -> matches (second condition)
        let test_data = [
            (1, true, "premium", 10u32),
            (2, false, "premium", 20),
            (3, true, "standard", 30),
            (4, false, "standard", 80),
            (5, false, "other", 90),
        ];

        for (i, active, cat, score) in test_data.iter() {
            let entity = IntegrationTestEntity::validation_builder()
                .id(format!("item-{i}"))
                .name(format!("Item {i}"))
                .score(*score)
                .category(cat.to_string())
                .active(*active)
                .created_at(now)
                .build()
                .expect("valid entity");

            snugom::run! {
                &repo,
                &mut conn,
                create => IntegrationTestEntity {
                    id: entity.id,
                    name: entity.name,
                    score: entity.score,
                    category: entity.category,
                    active: entity.active,
                    created_at: entity.created_at,
                }
            }
            .expect("create should succeed");
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Nested: (active=true AND category=premium) OR (score >= 80)
        let condition = FilterCondition::or([
            FilterCondition::and([
                FilterCondition::bool_eq("active", true),
                FilterCondition::tag_eq("category", "premium"),
            ]),
            FilterCondition::NumericRange {
                field: "score".to_string(),
                min: Some(80.0),
                max: None,
            },
        ]);

        let params = SearchParams::new()
            .with_condition(condition)
            .with_sort(Some(SearchSort {
                field: "score".to_string(),
                order: SortOrder::Asc,
            }));

        let result = repo.search(&mut conn, params).await.expect("search should succeed");

        // Should find items 1, 4, 5
        assert_eq!(result.total, 3, "should find exactly 3 items");

        let ids: Vec<&str> = result.items.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"item-1"), "item-1 (active+premium) should be returned");
        assert!(ids.contains(&"item-4"), "item-4 (score>=80) should be returned");
        assert!(ids.contains(&"item-5"), "item-5 (score>=80) should be returned");
        assert!(!ids.contains(&"item-2"), "item-2 should NOT be returned");
        assert!(!ids.contains(&"item-3"), "item-3 should NOT be returned");

        cleanup_keys(&mut conn, &format!("{prefix}:itest:items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:items:idx")).await;
    }

    // =========================================================================
    // INTEGRATION TESTS - Visibility Pattern (like KV store)
    // =========================================================================

    /// Entity with private/owner fields like KV store
    #[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
    #[snugom(version = 1)]
    pub struct VisibilityTestEntity {
        #[snugom(id)]
        pub id: String,

        #[snugom(searchable)]
        pub name: String,

        #[snugom(filterable)]
        pub private: bool,

        #[snugom(filterable(tag))]
        pub owner: String,

        #[snugom(datetime(epoch_millis), created_at)]
        pub created_at: DateTime<Utc>,
    }

    bundle! {
        service: "itest",
        entities: {
            IntegrationTestEntity => "items",
            VisibilityTestEntity => "visibility_items",
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_integration_visibility_pattern_private_false_or_owner_match() {
        use snugom::search::{FilterCondition, SearchParams};

        let mut conn = get_redis_connection().await;
        let prefix = "visibility_test";

        cleanup_keys(&mut conn, &format!("{prefix}:itest:visibility_items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:visibility_items:idx")).await;

        let repo: Repo<VisibilityTestEntity> = Repo::new(prefix.to_string());
        repo.ensure_search_index(&mut conn).await.expect("Failed to create index");

        let now = chrono::Utc::now();

        // Simulating KV visibility: user "alice" querying
        // Query: private=false OR owner="alice"
        //
        // - Item 1: private=false, owner=bob -> matches (public)
        // - Item 2: private=false, owner=alice -> matches (public, also alice's)
        // - Item 3: private=true, owner=alice -> matches (alice's private item)
        // - Item 4: private=true, owner=bob -> NOT (bob's private item, alice can't see)
        // - Item 5: private=true, owner=charlie -> NOT (charlie's private)
        let test_data = [
            (1, false, "bob"),
            (2, false, "alice"),
            (3, true, "alice"),
            (4, true, "bob"),
            (5, true, "charlie"),
        ];

        for (i, private, owner) in test_data.iter() {
            let entity = VisibilityTestEntity {
                id: format!("item-{i}"),
                name: format!("Item {i}"),
                private: *private,
                owner: owner.to_string(),
                created_at: now,
            };

            snugom::run! {
                &repo,
                &mut conn,
                create => VisibilityTestEntity {
                    id: entity.id,
                    name: entity.name,
                    private: entity.private,
                    owner: entity.owner,
                    created_at: entity.created_at,
                }
            }
            .expect("create should succeed");
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Visibility pattern: private=false OR owner=alice
        let visibility_condition = FilterCondition::or([
            FilterCondition::bool_eq("private", false),
            FilterCondition::tag_eq("owner", "alice"),
        ]);

        let params = SearchParams::new().with_condition(visibility_condition);

        let result = repo.search(&mut conn, params).await.expect("search should succeed");

        // Alice should see items 1, 2, 3 but NOT 4 or 5
        assert_eq!(result.total, 3, "alice should see exactly 3 items");

        let ids: Vec<&str> = result.items.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"item-1"), "item-1 (public) should be visible");
        assert!(ids.contains(&"item-2"), "item-2 (public, alice's) should be visible");
        assert!(ids.contains(&"item-3"), "item-3 (alice's private) should be visible");
        assert!(!ids.contains(&"item-4"), "item-4 (bob's private) should NOT be visible");
        assert!(!ids.contains(&"item-5"), "item-5 (charlie's private) should NOT be visible");

        // Now test as "bob" - should see items 1, 2, 4
        let bob_visibility = FilterCondition::or([
            FilterCondition::bool_eq("private", false),
            FilterCondition::tag_eq("owner", "bob"),
        ]);

        let bob_params = SearchParams::new().with_condition(bob_visibility);
        let bob_result = repo.search(&mut conn, bob_params).await.expect("search should succeed");

        assert_eq!(bob_result.total, 3, "bob should see exactly 3 items");
        let bob_ids: Vec<&str> = bob_result.items.iter().map(|i| i.id.as_str()).collect();
        assert!(bob_ids.contains(&"item-1"), "item-1 should be visible to bob");
        assert!(bob_ids.contains(&"item-2"), "item-2 should be visible to bob");
        assert!(bob_ids.contains(&"item-4"), "item-4 (bob's private) should be visible to bob");
        assert!(!bob_ids.contains(&"item-3"), "item-3 (alice's private) should NOT be visible to bob");

        cleanup_keys(&mut conn, &format!("{prefix}:itest:visibility_items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:visibility_items:idx")).await;
    }

    #[tokio::test]
    #[serial]
    async fn test_integration_visibility_with_additional_filters() {
        use snugom::search::{FilterCondition, SearchParams};

        let mut conn = get_redis_connection().await;
        let prefix = "visibility_test2";

        cleanup_keys(&mut conn, &format!("{prefix}:itest:visibility_items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:visibility_items:idx")).await;

        let repo: Repo<VisibilityTestEntity> = Repo::new(prefix.to_string());
        repo.ensure_search_index(&mut conn).await.expect("Failed to create index");

        let now = chrono::Utc::now();

        // Create items with different names for text search
        let test_data = [
            (1, "Config Database", false, "alice"),   // public, matches "config"
            (2, "Config Cache", true, "alice"),       // alice's private, matches "config"
            (3, "Settings File", false, "bob"),       // public, doesn't match "config"
            (4, "Config Server", true, "bob"),        // bob's private, matches "config"
        ];

        for (i, name, private, owner) in test_data.iter() {
            let entity = VisibilityTestEntity {
                id: format!("item-{i}"),
                name: name.to_string(),
                private: *private,
                owner: owner.to_string(),
                created_at: now,
            };

            snugom::run! {
                &repo,
                &mut conn,
                create => VisibilityTestEntity {
                    id: entity.id,
                    name: entity.name,
                    private: entity.private,
                    owner: entity.owner,
                    created_at: entity.created_at,
                }
            }
            .expect("create should succeed");
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Alice searches for "config" with visibility filter
        // Should see: item-1 (public config), item-2 (her private config)
        // Should NOT see: item-4 (bob's private config)
        let visibility_condition = FilterCondition::or([
            FilterCondition::bool_eq("private", false),
            FilterCondition::tag_eq("owner", "alice"),
        ]);

        let params = SearchParams::new()
            .with_condition(visibility_condition)
            .with_text_query("config");

        let result = repo.search(&mut conn, params).await.expect("search should succeed");

        assert_eq!(result.total, 2, "alice should find 2 config items");

        let ids: Vec<&str> = result.items.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"item-1"), "item-1 (public config) should be found");
        assert!(ids.contains(&"item-2"), "item-2 (alice's private config) should be found");
        assert!(!ids.contains(&"item-3"), "item-3 (no 'config') should NOT be found");
        assert!(!ids.contains(&"item-4"), "item-4 (bob's private) should NOT be found");

        cleanup_keys(&mut conn, &format!("{prefix}:itest:visibility_items:*")).await;
        drop_index_if_exists(&mut conn, &format!("{prefix}:itest:visibility_items:idx")).await;
    }
}
