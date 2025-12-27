//! # RediSearch Query Building and Escaping
//!
//! This module provides utilities for building RediSearch queries with proper escaping.
//!
//! ## Escaping Functions Quick Reference
//!
//! | Function                        | Input           | Output           | Use Case                     |
//! |---------------------------------|-----------------|------------------|------------------------------|
//! | `escape_for_tag_query(value)`   | `"test-user"`   | `"test\-user"`   | TAG field: `@field:{...}`    |
//! | `escape_for_text_prefix(value)` | `"cli-kv/data"` | `"cli kv data*"` | Tokenizes + wildcards last   |
//! | `escape_for_text_contains(value)`| `"hello"`      | `"*hello*"`      | Wraps with `*` for contains  |
//! | `escape_for_text_exact(value)`  | `"John Doe"`    | `"\"John Doe\""` | Wraps with quotes for exact  |
//! | `escape_for_text_fuzzy(value)`  | `"wrold"`       | `"%wrold%"`      | Wraps with `%` for fuzzy     |
//! | `escape_for_text_search(term)`  | `"dragon"`      | `"dragon*"`      | Adds trailing `*` for search |
//!
//! ## Why Different Functions?
//!
//! RediSearch has two field types with different escaping needs:
//!
//! - **TAG fields**: Exact matching. Must escape `$ { } \ | - .` (hyphen is NOT, period is JSON path)
//! - **TEXT fields**: Tokenized at index time on `-` and `/`. Query escaping must match
//!   the tokenization that occurred at index time.
//!
//! ## Example Usage
//!
//! ```text
//! // TAG field query (exact match)
//! let owner = escape_for_tag_query("test-user");
//! // => @owner:{test\-user}
//!
//! // TEXT field prefix query (path matching)
//! let path = escape_for_text_prefix("config/db-settings");
//! // => @path:config db settings*
//!
//! // TEXT field contains query
//! let desc = escape_for_text_contains("error");
//! // => @desc:*error*
//! ```

use redis::{Value, aio::ConnectionManager, cmd, from_redis_value};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value as JsonValue;
use std::borrow::Cow;

#[cfg(feature = "utoipa")]
use utoipa::ToSchema;

use crate::{errors::RepoError, types::EntityMetadata};

const DEFAULT_PAGE: u64 = 1;
const DEFAULT_PAGE_SIZE: u64 = 25;
const MAX_PAGE_SIZE: u64 = 100;
const TAG_SEPARATOR: &str = "|";

/// Trait implemented by entities that expose SnugOM search metadata.
pub trait SearchEntity: EntityMetadata + DeserializeOwned {
    /// Return the RediSearch index definition for the entity. The provided prefix is the
    /// global key prefix (e.g., `snug`), mirroring the legacy manager behaviour.
    fn index_definition(prefix: &str) -> IndexDefinition;

    /// List of allowed sort fields.
    fn allowed_sorts() -> &'static [SortField];

    /// Default sort field when none is supplied.
    fn default_sort() -> &'static SortField;

    /// Fields used for full-text searches.
    fn text_search_fields() -> &'static [&'static str];

    /// Map an incoming filter descriptor to a filter condition.
    fn map_filter(descriptor: FilterDescriptor) -> Result<FilterCondition, RepoError>;

    /// Base filter applied automatically to every search.
    fn base_filter() -> String {
        String::new()
    }
}

#[cfg_attr(feature = "utoipa", derive(ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    #[default]
    Asc,
    Desc,
}

impl SortOrder {
    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            SortOrder::Asc => "ASC",
            SortOrder::Desc => "DESC",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SortField {
    pub name: &'static str,
    pub path: &'static str,
    pub default_order: SortOrder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterOperator {
    Eq,
    Range,
    Bool,
    Prefix,
    Contains,
    Exact,
    Fuzzy,
}

#[derive(Debug, Clone)]
pub struct FilterDescriptor {
    pub field: String,
    pub operator: FilterOperator,
    pub values: Vec<String>,
}

/// A composable filter condition for RediSearch queries.
///
/// Leaf conditions represent individual field filters, while `And` and `Or`
/// allow building complex boolean expressions.
///
/// # Examples
///
/// ```
/// use snugom::search::FilterCondition;
///
/// // Simple equality
/// let status = FilterCondition::tag_eq("status", "active");
///
/// // OR combination (visibility pattern)
/// let visibility = FilterCondition::or([
///     FilterCondition::bool_eq("private", false),
///     FilterCondition::tag_eq("owner", "user123"),
/// ]);
///
/// // Complex: (active AND priority > 5) OR owned by user
/// let complex = FilterCondition::or([
///     FilterCondition::and([
///         FilterCondition::tag_eq("status", "active"),
///         FilterCondition::numeric_gt("priority", 5.0),
///     ]),
///     FilterCondition::tag_eq("owner", "user123"),
/// ]);
/// ```
#[derive(Debug, Clone)]
pub enum FilterCondition {
    // Leaf conditions
    TagEquals {
        field: String,
        values: Vec<String>,
    },
    NumericRange {
        field: String,
        min: Option<f64>,
        max: Option<f64>,
    },
    BooleanEquals {
        field: String,
        value: bool,
    },
    TextPrefix {
        field: String,
        value: String,
    },
    TextContains {
        field: String,
        value: String,
    },
    TextExact {
        field: String,
        value: String,
    },
    TextFuzzy {
        field: String,
        value: String,
    },
    // Composite conditions
    And(Vec<FilterCondition>),
    Or(Vec<FilterCondition>),
}

impl FilterCondition {
    // ========== Leaf Constructors ==========

    /// Create a TAG field equality filter for a single value.
    #[inline]
    pub fn tag_eq(field: impl Into<String>, value: impl Into<String>) -> Self {
        Self::TagEquals {
            field: field.into(),
            values: vec![value.into()],
        }
    }

    /// Create a TAG field filter matching any of the given values (OR within field).
    #[inline]
    pub fn tag_in<S: Into<String>>(field: impl Into<String>, values: impl IntoIterator<Item = S>) -> Self {
        Self::TagEquals {
            field: field.into(),
            values: values.into_iter().map(Into::into).collect(),
        }
    }

    /// Create a boolean field equality filter.
    #[inline]
    pub fn bool_eq(field: impl Into<String>, value: bool) -> Self {
        Self::BooleanEquals {
            field: field.into(),
            value,
        }
    }

    /// Create a numeric range filter (inclusive bounds).
    #[inline]
    pub fn numeric_range(field: impl Into<String>, min: Option<f64>, max: Option<f64>) -> Self {
        Self::NumericRange {
            field: field.into(),
            min,
            max,
        }
    }

    /// Create a numeric "greater than" filter.
    #[inline]
    pub fn numeric_gt(field: impl Into<String>, min: f64) -> Self {
        Self::NumericRange {
            field: field.into(),
            min: Some(min),
            max: None,
        }
    }

    /// Create a numeric "less than" filter.
    #[inline]
    pub fn numeric_lt(field: impl Into<String>, max: f64) -> Self {
        Self::NumericRange {
            field: field.into(),
            min: None,
            max: Some(max),
        }
    }

    /// Create a numeric equality filter.
    #[inline]
    pub fn numeric_eq(field: impl Into<String>, value: f64) -> Self {
        Self::NumericRange {
            field: field.into(),
            min: Some(value),
            max: Some(value),
        }
    }

    /// Create a TEXT field prefix filter.
    #[inline]
    pub fn text_prefix(field: impl Into<String>, value: impl Into<String>) -> Self {
        Self::TextPrefix {
            field: field.into(),
            value: value.into(),
        }
    }

    /// Create a TEXT field contains filter.
    #[inline]
    pub fn text_contains(field: impl Into<String>, value: impl Into<String>) -> Self {
        Self::TextContains {
            field: field.into(),
            value: value.into(),
        }
    }

    /// Create a TEXT field exact phrase filter.
    #[inline]
    pub fn text_exact(field: impl Into<String>, value: impl Into<String>) -> Self {
        Self::TextExact {
            field: field.into(),
            value: value.into(),
        }
    }

    /// Create a TEXT field fuzzy filter.
    #[inline]
    pub fn text_fuzzy(field: impl Into<String>, value: impl Into<String>) -> Self {
        Self::TextFuzzy {
            field: field.into(),
            value: value.into(),
        }
    }

    // ========== Composite Constructors ==========

    /// Combine conditions with AND logic.
    #[inline]
    pub fn and(conditions: impl IntoIterator<Item = FilterCondition>) -> Self {
        Self::And(conditions.into_iter().collect())
    }

    /// Combine conditions with OR logic.
    #[inline]
    pub fn or(conditions: impl IntoIterator<Item = FilterCondition>) -> Self {
        Self::Or(conditions.into_iter().collect())
    }

    // ========== Query Generation ==========

    /// Convert this condition to a RediSearch query clause.
    pub fn to_query_clause(&self) -> String {
        match self {
            Self::TagEquals { field, values } => {
                let escaped: Vec<String> = values.iter().map(|v| escape_for_tag_query(v)).collect();
                format!("(@{}:{{{}}})", field, escaped.join(TAG_SEPARATOR))
            }
            Self::NumericRange { field, min, max } => {
                let min_s = min.map(format_numeric).unwrap_or_else(|| "-inf".to_string());
                let max_s = max.map(format_numeric).unwrap_or_else(|| "+inf".to_string());
                format!("(@{}:[{} {}])", field, min_s, max_s)
            }
            Self::BooleanEquals { field, value } => {
                let normalized = if *value { "true" } else { "false" };
                format!("(@{}:{{{}}})", field, normalized)
            }
            Self::TextPrefix { field, value } => {
                format!("(@{}:{})", field, escape_for_text_prefix(value))
            }
            Self::TextContains { field, value } => {
                format!("(@{}:{})", field, escape_for_text_contains(value))
            }
            Self::TextExact { field, value } => {
                format!("(@{}:{})", field, escape_for_text_exact(value))
            }
            Self::TextFuzzy { field, value } => {
                format!("(@{}:{})", field, escape_for_text_fuzzy(value))
            }
            Self::And(conditions) => {
                if conditions.is_empty() {
                    return String::new();
                }
                let clauses: Vec<String> = conditions
                    .iter()
                    .map(|c| c.to_query_clause())
                    .filter(|s| !s.is_empty())
                    .collect();
                match clauses.len() {
                    0 => String::new(),
                    1 => clauses.into_iter().next().unwrap_or_default(),
                    _ => format!("({})", clauses.join(" ")),
                }
            }
            Self::Or(conditions) => {
                if conditions.is_empty() {
                    return String::new();
                }
                let clauses: Vec<String> = conditions
                    .iter()
                    .map(|c| c.to_query_clause())
                    .filter(|s| !s.is_empty())
                    .collect();
                match clauses.len() {
                    0 => String::new(),
                    1 => clauses.into_iter().next().unwrap_or_default(),
                    _ => format!("({})", clauses.join("|")),
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchSort {
    pub field: String,
    pub order: SortOrder,
}

/// Search parameters for RediSearch queries.
///
/// # Building Queries
///
/// Use the builder methods to construct search parameters:
///
/// ```
/// use snugom::search::{SearchParams, FilterCondition};
///
/// let params = SearchParams::new()
///     .with_condition(FilterCondition::or([
///         FilterCondition::bool_eq("private", false),
///         FilterCondition::tag_eq("owner", "user123"),
///     ]))
///     .with_condition(FilterCondition::tag_eq("status", "active"))
///     .with_page(1, 25);
/// ```
#[derive(Debug, Clone)]
pub struct SearchParams {
    pub page: u64,
    pub page_size: u64,
    pub sort: Option<SearchSort>,
    /// Filter conditions (composed via And/Or). All conditions are ANDed at top level.
    pub conditions: Vec<FilterCondition>,
    /// Free-text search query (generates proper multi-field search).
    pub text_query: Option<String>,
    /// Raw RediSearch query escape hatch. Use sparingly.
    pub raw: Option<String>,
}

impl Default for SearchParams {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchParams {
    pub fn new() -> Self {
        Self {
            page: DEFAULT_PAGE,
            page_size: DEFAULT_PAGE_SIZE,
            sort: None,
            conditions: Vec::new(),
            text_query: None,
            raw: None,
        }
    }

    #[inline]
    pub fn offset(&self) -> u64 {
        self.page.saturating_sub(1) * self.page_size
    }

    #[inline]
    pub fn with_sort(mut self, sort: Option<SearchSort>) -> Self {
        self.sort = sort;
        self
    }

    /// Add a single filter condition (leaf or composed).
    #[inline]
    pub fn with_condition(mut self, condition: FilterCondition) -> Self {
        self.conditions.push(condition);
        self
    }

    /// Add multiple filter conditions.
    #[inline]
    pub fn with_conditions(mut self, conditions: impl IntoIterator<Item = FilterCondition>) -> Self {
        self.conditions.extend(conditions);
        self
    }

    /// Set the free-text search query.
    #[inline]
    pub fn with_text_query(mut self, query: impl Into<String>) -> Self {
        self.text_query = Some(query.into());
        self
    }

    /// Set a raw RediSearch query clause (escape hatch - use sparingly).
    #[inline]
    pub fn with_raw(mut self, raw: impl Into<String>) -> Self {
        self.raw = Some(raw.into());
        self
    }

    #[inline]
    pub fn with_page(mut self, page: u64, page_size: u64) -> Self {
        self.page = page;
        self.page_size = page_size;
        self
    }

    pub fn build_query(&self, base: &str) -> String {
        let estimated_capacity = 3 + self.conditions.len();
        let mut clauses = Vec::with_capacity(estimated_capacity);

        // Entity base filter (e.g., tenant scoping)
        if !base.is_empty() {
            clauses.push(format!("({})", base));
        }

        // Filter conditions (composed FilterCondition)
        for condition in &self.conditions {
            let clause = condition.to_query_clause();
            if !clause.is_empty() {
                clauses.push(clause);
            }
        }

        // Free-text search query
        if let Some(q) = &self.text_query
            && !q.is_empty()
        {
            clauses.push(format!("({})", q));
        }

        // Raw escape hatch (last)
        if let Some(raw) = &self.raw
            && !raw.is_empty()
        {
            clauses.push(format!("({})", raw));
        }

        if clauses.is_empty() {
            "*".to_string()
        } else {
            // Pre-calculate capacity for the final joined string
            let total_len: usize = clauses.iter().map(|s| s.len()).sum();
            let capacity = total_len + clauses.len() - 1;

            let mut result = String::with_capacity(capacity);
            for (i, clause) in clauses.iter().enumerate() {
                if i > 0 {
                    result.push(' ');
                }
                result.push_str(clause);
            }
            result
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u64,
    pub page_size: u64,
    pub has_more: bool,
}

#[derive(Debug, Clone)]
pub struct SearchResult<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u64,
    pub page_size: u64,
}

impl<T> SearchResult<T> {
    #[inline]
    pub fn has_more(&self) -> bool {
        self.page * self.page_size < self.total
    }
}

impl<T: Serialize> From<SearchResult<T>> for PaginatedResponse<T> {
    fn from(value: SearchResult<T>) -> Self {
        Self {
            has_more: value.has_more(),
            page: value.page,
            page_size: value.page_size,
            total: value.total,
            items: value.items,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SearchQuery {
    pub page: Option<u64>,
    #[serde(rename = "page_size")]
    pub page_size: Option<u64>,
    pub sort_by: Option<String>,
    pub sort_order: Option<SortOrder>,
    pub q: Option<String>,
    #[serde(default)]
    pub filter: Vec<String>,
}

impl SearchQuery {
    /// Parse query parameters into SearchParams using a filter mapper.
    ///
    /// The filter_mapper converts parsed filter descriptors into FilterConditions.
    /// This is typically provided by entity implementations via `T::map_filter`.
    #[allow(clippy::too_many_arguments)]
    pub fn into_params<F>(
        self,
        allowed_sorts: &[SortField],
        default_sort: &SortField,
        mut filter_mapper: F,
    ) -> Result<SearchParams, RepoError>
    where
        F: FnMut(FilterDescriptor) -> Result<FilterCondition, RepoError>,
    {
        let requested_size = self.page_size.unwrap_or(DEFAULT_PAGE_SIZE);
        let page_size = requested_size.clamp(1, MAX_PAGE_SIZE);

        let page = self.page.unwrap_or(DEFAULT_PAGE).max(1);

        let sort_field = if let Some(sort_name) = self.sort_by.as_deref() {
            allowed_sorts
                .iter()
                .find(|field| field.name.eq_ignore_ascii_case(sort_name))
                .copied()
                .ok_or_else(|| RepoError::InvalidRequest {
                    message: format!("Unsupported sort field: {}", sort_name),
                })?
        } else {
            *default_sort
        };

        let sort_order = self.sort_order.unwrap_or(sort_field.default_order);
        let sort = Some(SearchSort {
            field: sort_field.path.to_string(),
            order: sort_order,
        });

        let mut conditions = Vec::new();
        for raw in self.filter {
            let parts: Vec<&str> = raw.splitn(3, ':').collect();
            if parts.len() != 3 {
                return Err(RepoError::InvalidRequest {
                    message: format!("Invalid filter syntax: {}", raw),
                });
            }

            let operator = match parts[1].to_ascii_lowercase().as_str() {
                "eq" => FilterOperator::Eq,
                "range" => FilterOperator::Range,
                "bool" | "boolean" => FilterOperator::Bool,
                "prefix" => FilterOperator::Prefix,
                "contains" => FilterOperator::Contains,
                "exact" => FilterOperator::Exact,
                "fuzzy" => FilterOperator::Fuzzy,
                other => {
                    return Err(RepoError::InvalidRequest {
                        message: format!("Unsupported filter operator: {}", other),
                    });
                }
            };

            let values = match operator {
                FilterOperator::Eq | FilterOperator::Bool => parts[2]
                    .split(['|', ','])
                    .filter(|segment| !segment.is_empty())
                    .map(|segment| segment.trim().to_string())
                    .collect(),
                FilterOperator::Range => parts[2].split(',').map(|segment| segment.trim().to_string()).collect(),
                // TEXT field filters take a single value (no splitting)
                FilterOperator::Prefix | FilterOperator::Contains | FilterOperator::Exact | FilterOperator::Fuzzy => {
                    vec![parts[2].to_string()]
                }
            };

            let descriptor = FilterDescriptor {
                field: parts[0].trim().to_string(),
                operator,
                values,
            };

            conditions.push(filter_mapper(descriptor)?);
        }

        Ok(SearchParams::new()
            .with_page(page, page_size)
            .with_sort(sort)
            .with_conditions(conditions))
    }

    /// Parse query with free-text search support.
    ///
    /// The `q` parameter is tokenized and searched across the specified text fields.
    pub fn with_text_query<F>(
        self,
        allowed_sorts: &[SortField],
        default_sort: &SortField,
        filter_mapper: F,
        text_fields: &[&str],
    ) -> Result<SearchParams, RepoError>
    where
        F: FnMut(FilterDescriptor) -> Result<FilterCondition, RepoError>,
    {
        let text_term = self.q.clone();
        let mut params = self.into_params(allowed_sorts, default_sort, filter_mapper)?;
        params.text_query = build_text_query(text_term, text_fields);
        Ok(params)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum IndexFieldType {
    Tag,
    Text,
    Numeric,
    Geo,
}

#[derive(Debug, Clone, Copy)]
pub struct IndexField {
    pub path: &'static str,
    pub field_name: &'static str,
    pub field_type: IndexFieldType,
    pub sortable: bool,
}

#[derive(Debug, Clone)]
pub struct IndexDefinition {
    pub name: String,
    pub prefixes: Vec<String>,
    pub filter: Option<String>,
    pub schema: &'static [IndexField],
}

pub async fn ensure_index(conn: &mut ConnectionManager, definition: &IndexDefinition) -> Result<(), RepoError> {
    let indexes: Vec<String> = cmd("FT._LIST").query_async(conn).await?;
    if indexes.iter().any(|name| name == &definition.name) {
        return Ok(());
    }

    let mut command = cmd("FT.CREATE");
    command.arg(definition.name.as_str());
    command.arg("ON").arg("JSON");
    command.arg("PREFIX").arg(definition.prefixes.len());
    for prefix in &definition.prefixes {
        command.arg(prefix.as_str());
    }

    if let Some(filter) = &definition.filter {
        command.arg("FILTER").arg(filter.as_str());
    }

    command.arg("SCHEMA");
    for field in definition.schema {
        command.arg(field.path);
        command.arg("AS").arg(field.field_name);
        match field.field_type {
            IndexFieldType::Tag => {
                command.arg("TAG");
                command.arg("SEPARATOR").arg(TAG_SEPARATOR);
            }
            IndexFieldType::Text => {
                command.arg("TEXT");
            }
            IndexFieldType::Numeric => {
                command.arg("NUMERIC");
            }
            IndexFieldType::Geo => {
                command.arg("GEO");
            }
        }

        if field.sortable {
            command.arg("SORTABLE");
        }
    }

    if let Err(err) = command.query_async::<()>(conn).await {
        if index_exists_error(&err) {
            return Ok(());
        }
        return Err(err.into());
    }

    Ok(())
}

fn index_exists_error(err: &redis::RedisError) -> bool {
    let msg = err.to_string().to_ascii_lowercase();
    msg.contains("already exists") && msg.contains("index")
}

#[allow(async_fn_in_trait)]
pub trait SearchableManager {
    type Item: DeserializeOwned + Send + Sync;

    fn index_definition(&self) -> IndexDefinition;

    fn allowed_sorts(&self) -> &'static [SortField];

    fn default_sort(&self) -> &'static SortField;

    fn text_search_fields(&self) -> &'static [&'static str];

    fn base_filter(&self) -> String {
        String::new()
    }

    async fn ensure_index(&self, conn: &mut ConnectionManager) -> Result<(), RepoError> {
        let definition = self.index_definition();
        ensure_index(conn, &definition).await
    }

    async fn search(
        &self,
        conn: &mut ConnectionManager,
        params: SearchParams,
    ) -> Result<SearchResult<Self::Item>, RepoError> {
        let definition = self.index_definition();
        execute_search(conn, definition.name.as_ref(), &params, &self.base_filter()).await
    }
}

pub async fn execute_search<T>(
    conn: &mut ConnectionManager,
    index_name: &str,
    params: &SearchParams,
    base_query: &str,
) -> Result<SearchResult<T>, RepoError>
where
    T: DeserializeOwned,
{
    let query = params.build_query(base_query);

    let mut command = cmd("FT.SEARCH");
    command.arg(index_name);
    command.arg(query);

    if let Some(sort) = &params.sort {
        command.arg("SORTBY").arg(&sort.field).arg(sort.order.as_str());
    }

    let start = params.offset();
    let count = params.page_size;
    command.arg("LIMIT").arg(start).arg(count);
    command.arg("RETURN").arg(1).arg("$");
    command.arg("DIALECT").arg(3);

    let raw: Value = command.query_async(conn).await?;
    let values: Vec<Value> = from_redis_value(&raw).map_err(|err| RepoError::Other {
        message: Cow::Owned(format!("Failed to parse search response: {}", err)),
    })?;

    if values.is_empty() {
        return Ok(SearchResult {
            items: Vec::new(),
            total: 0,
            page: params.page,
            page_size: params.page_size,
        });
    }

    let total = match &values[0] {
        Value::Int(v) => *v as u64,
        Value::BulkString(bytes) => String::from_utf8(bytes.clone())
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| RepoError::Other {
                message: Cow::Owned("Invalid total count in search response".to_string()),
            })?,
        other => {
            let repr = format!("{:?}", other);
            return Err(RepoError::Other {
                message: Cow::Owned(format!("Unexpected total count type: {}", repr)),
            });
        }
    };

    let mut items = Vec::new();
    let mut idx = 1;
    while idx + 1 < values.len() {
        let doc_value = &values[idx + 1];
        let json_payload = extract_json_payload(doc_value)?;
        let item: T = serde_json::from_str(&json_payload).map_err(|err| RepoError::Other {
            message: Cow::Owned(format!("Failed to deserialize search document: {}", err)),
        })?;
        items.push(item);
        idx += 2;
    }

    Ok(SearchResult {
        items,
        total,
        page: params.page,
        page_size: params.page_size,
    })
}

pub fn build_text_query(term: Option<String>, fields: &[&str]) -> Option<String> {
    let raw = term?.trim().to_string();
    if raw.is_empty() {
        return None;
    }

    let tokens: Vec<String> = raw
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .map(escape_for_text_search)
        .collect();

    if tokens.is_empty() {
        return None;
    }

    let joined_tokens = tokens.join(" ");
    let field_queries: Vec<String> = fields.iter().map(|field| format!("@{}:({})", field, joined_tokens)).collect();

    Some(format!("({})", field_queries.join(" | ")))
}

fn extract_json_payload(value: &Value) -> Result<String, RepoError> {
    match value {
        Value::Array(items) => {
            let iter = items.chunks(2);
            for chunk in iter {
                if chunk.len() != 2 {
                    continue;
                }

                let alias: String = from_redis_value(&chunk[0]).map_err(|err| RepoError::Other {
                    message: Cow::Owned(format!("Invalid field alias in search document: {}", err)),
                })?;

                if alias == "doc" || alias == "$" {
                    let payload = value_to_string(&chunk[1])?;
                    return normalize_json_payload(payload);
                }
            }

            Err(RepoError::Other {
                message: Cow::Owned("Search response missing JSON payload".to_string()),
            })
        }
        other => normalize_json_payload(value_to_string(other)?),
    }
}

fn normalize_json_payload(mut payload: String) -> Result<String, RepoError> {
    let trimmed = payload.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let value: JsonValue = serde_json::from_str(trimmed).map_err(|err| RepoError::Other {
            message: Cow::Owned(format!("Failed to parse JSON payload array: {}", err)),
        })?;
        if let Some(first) = value.as_array().and_then(|arr| arr.first()) {
            payload = serde_json::to_string(first).map_err(|err| RepoError::Other {
                message: Cow::Owned(format!("Failed to serialize JSON payload element: {}", err)),
            })?;
        }
    }
    Ok(payload)
}

fn value_to_string(value: &Value) -> Result<String, RepoError> {
    match value {
        Value::BulkString(bytes) => String::from_utf8(bytes.clone()).map_err(|err| RepoError::Other {
            message: Cow::Owned(format!("Invalid UTF-8 in search response: {}", err)),
        }),
        Value::SimpleString(status) => Ok(status.clone()),
        Value::Int(v) => Ok(v.to_string()),
        Value::Double(v) => Ok(v.to_string()),
        Value::Boolean(v) => Ok(v.to_string()),
        Value::VerbatimString { text, .. } => Ok(text.clone()),
        _ => from_redis_value::<String>(value).map_err(|err| RepoError::Other {
            message: Cow::Owned(format!("Unexpected search value type: {}", err)),
        }),
    }
}

/// Escape a value for RediSearch TAG field queries.
///
/// TAG fields use exact matching. This function escapes characters that have
/// special meaning in RediSearch query syntax.
///
/// # Characters Escaped
/// - `$` - Variable prefix in some contexts
/// - `{`, `}` - TAG value delimiters
/// - `\` - Escape character itself
/// - `|` - OR operator in TAG queries
/// - `-` - NOT operator in query syntax
/// - `.` - JSON path separator in RediSearch
///
/// # Characters NOT Escaped
/// Spaces, colons, brackets, and quotes are allowed in TAG values without escaping.
///
/// # Examples
///
/// ```
/// use snugom::search::escape_for_tag_query;
///
/// // Simple values pass through unchanged
/// assert_eq!(escape_for_tag_query("active"), "active");
/// assert_eq!(escape_for_tag_query("New York"), "New York");
///
/// // Hyphens are escaped (NOT operator)
/// assert_eq!(escape_for_tag_query("test-user"), "test\\-user");
///
/// // Pipes are escaped (OR operator)
/// assert_eq!(escape_for_tag_query("a|b"), "a\\|b");
///
/// // Dollar signs are escaped
/// assert_eq!(escape_for_tag_query("$100"), "\\$100");
///
/// // Braces are escaped
/// assert_eq!(escape_for_tag_query("{foo}"), "\\{foo\\}");
///
/// // Periods are escaped (JSON path separator)
/// assert_eq!(escape_for_tag_query("list.test"), "list\\.test");
///
/// // Combined escaping
/// assert_eq!(escape_for_tag_query("test-user|admin"), "test\\-user\\|admin");
/// ```
///
/// # Usage in Queries
///
/// This function is used internally when building TAG field queries:
/// ```text
/// Input filter:  "owner:eq:test-user"
/// Escaped value: "test\-user"
/// Generated query: @owner:{test\-user}
/// ```
pub fn escape_for_tag_query(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            // Core TAG escaping per docs (including . which is JSON path separator)
            '$' | '{' | '}' | '\\' | '|' | '.' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            // Hyphen must also be escaped - it's the NOT operator in query syntax
            '-' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// Escape and format a value for RediSearch TEXT prefix queries.
///
/// Tokenizes the value on `-` and `/` (matching RediSearch's index-time tokenization),
/// escapes each token, and adds a wildcard `*` to the last token for prefix matching.
///
/// # Examples
///
/// ```
/// use snugom::search::escape_for_text_prefix;
///
/// // Simple value gets wildcard
/// assert_eq!(escape_for_text_prefix("config"), "config*");
///
/// // Path is tokenized, wildcard on last
/// assert_eq!(escape_for_text_prefix("cli-kv/data"), "cli kv data*");
/// assert_eq!(escape_for_text_prefix("config/db-settings"), "config db settings*");
///
/// // Trailing separators are handled
/// assert_eq!(escape_for_text_prefix("config/db/"), "config db*");
///
/// // Special chars in tokens are escaped
/// assert_eq!(escape_for_text_prefix("user:name"), "user\\:name*");
/// ```
///
/// # Why Tokenization is Needed
///
/// RediSearch TEXT fields tokenize on `-` and `/` at index time. If we query
/// `@path:cli-kv-tests*`, the `-` is interpreted as NOT, excluding results.
/// By tokenizing ourselves, we match the index structure.
pub fn escape_for_text_prefix(value: &str) -> String {
    let tokens: Vec<&str> = value.split(['-', '/']).filter(|s| !s.is_empty()).collect();

    if tokens.is_empty() {
        return "*".to_string();
    }

    let mut parts: Vec<String> = tokens
        .iter()
        .take(tokens.len().saturating_sub(1))
        .map(|t| escape_text_token(t))
        .collect();

    if let Some(last) = tokens.last() {
        parts.push(format!("{}*", escape_text_token(last)));
    }

    parts.join(" ")
}

/// Escape and format a value for RediSearch TEXT contains queries.
///
/// Escapes special characters and wraps the value in `*...*` for substring matching.
///
/// # Examples
///
/// ```
/// use snugom::search::escape_for_text_contains;
///
/// // Simple value wrapped with wildcards
/// assert_eq!(escape_for_text_contains("hello"), "*hello*");
/// assert_eq!(escape_for_text_contains("error"), "*error*");
///
/// // Special chars are escaped
/// assert_eq!(escape_for_text_contains("name@domain"), "*name\\@domain*");
/// assert_eq!(escape_for_text_contains("50%"), "*50\\%*");
/// ```
pub fn escape_for_text_contains(value: &str) -> String {
    format!("*{}*", escape_text_value(value))
}

/// Escape and format a value for RediSearch TEXT exact phrase queries.
///
/// Escapes quotes and backslashes, then wraps the value in double quotes
/// for exact phrase matching.
///
/// # Examples
///
/// ```
/// use snugom::search::escape_for_text_exact;
///
/// // Simple phrase wrapped in quotes
/// assert_eq!(escape_for_text_exact("hello world"), "\"hello world\"");
/// assert_eq!(escape_for_text_exact("John Doe"), "\"John Doe\"");
///
/// // Quotes in value are escaped
/// assert_eq!(escape_for_text_exact("say \"hello\""), "\"say \\\"hello\\\"\"");
///
/// // Backslashes are escaped
/// assert_eq!(escape_for_text_exact("C:\\Users"), "\"C:\\\\Users\"");
/// ```
pub fn escape_for_text_exact(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '\\' | '"' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}

/// Escape and format a value for RediSearch TEXT fuzzy queries.
///
/// Escapes special characters and wraps the value in `%...%` for fuzzy/Levenshtein matching.
///
/// # Examples
///
/// ```
/// use snugom::search::escape_for_text_fuzzy;
///
/// // Simple value wrapped with fuzzy markers
/// assert_eq!(escape_for_text_fuzzy("hello"), "%hello%");
/// assert_eq!(escape_for_text_fuzzy("wrold"), "%wrold%");  // typo for "world"
///
/// // Special chars are escaped
/// assert_eq!(escape_for_text_fuzzy("test%value"), "%test\\%value%");
/// ```
pub fn escape_for_text_fuzzy(value: &str) -> String {
    format!("%{}%", escape_text_value(value))
}

/// Escape and format a search term for RediSearch free-text search.
///
/// Escapes special characters and adds a trailing `*` wildcard for prefix matching.
/// Use this for search box input where each word should match as a prefix.
///
/// # Examples
///
/// ```
/// use snugom::search::escape_for_text_search;
///
/// // Adds wildcard to each term
/// assert_eq!(escape_for_text_search("dragon"), "dragon*");
/// assert_eq!(escape_for_text_search("hello"), "hello*");
///
/// // Special chars escaped, wildcard added
/// assert_eq!(escape_for_text_search("user:test"), "user\\:test*");
/// ```
///
/// # Usage
///
/// ```text
/// User types: "dragon knight"
/// Split on whitespace, then for each term:
///   escape_for_text_search("dragon") -> "dragon*"
///   escape_for_text_search("knight") -> "knight*"
/// Build query: (@name:(dragon* knight*)) | (@desc:(dragon* knight*))
/// ```
pub fn escape_for_text_search(term: &str) -> String {
    let mut escaped = escape_text_token(term);
    escaped.push('*');
    escaped
}

// ============================================================================
// Internal helper functions (not part of public API)
// ============================================================================

/// Internal: Escape special characters for TEXT field values.
/// Used by escape_for_text_contains and escape_for_text_fuzzy.
fn escape_text_value(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            // RediSearch special characters that need escaping in TEXT field queries.
            // Note: '-' and '/' are NOT escaped because they are tokenizers in TEXT fields.
            '\\' | '(' | ')' | '|' | '\'' | '"' | '[' | ']' | '{' | '}' | ':' | '@' | '?' | '~' | '&' | '!' | '.'
            | '*' | '%' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// Internal: Escape a single token for TEXT field queries.
/// Used by escape_for_text_prefix and escape_for_text_search.
fn escape_text_token(token: &str) -> String {
    let mut escaped = String::with_capacity(token.len());
    for ch in token.chars() {
        match ch {
            // Escape RediSearch query special characters within a token.
            // Note: '*' and '%' are NOT escaped here - caller adds wildcards/fuzzy markers.
            '\\' | '(' | ')' | '|' | '\'' | '"' | '[' | ']' | '{' | '}' | ':' | '@' | '?' | '~' | '&' | '!' | '.' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn format_numeric(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{:.0}", value)
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_filter_mapper(descriptor: FilterDescriptor) -> Result<FilterCondition, RepoError> {
        match descriptor.field.as_str() {
            "visibility" => {
                assert_eq!(descriptor.operator, FilterOperator::Eq);
                if descriptor.values.is_empty() {
                    return Err(RepoError::InvalidRequest {
                        message: "visibility filter requires a value".to_string(),
                    });
                }
                Ok(FilterCondition::TagEquals {
                    field: "visibility".to_string(),
                    values: descriptor.values,
                })
            }
            "member_count" => crate::filters::normalizers::build_numeric_filter(descriptor, "member_count"),
            "created_at" | "created_at_ts" => {
                crate::filters::normalizers::build_numeric_filter(descriptor, "created_at_ts")
            }
            "active" => {
                assert_eq!(descriptor.operator, FilterOperator::Bool);
                let value = descriptor.values.get(0).ok_or_else(|| RepoError::InvalidRequest {
                    message: "active filter requires a value".to_string(),
                })?;
                let flag = match value.as_str() {
                    "true" | "True" => true,
                    "false" | "False" => false,
                    other => {
                        return Err(RepoError::InvalidRequest {
                            message: format!("Invalid boolean value for active: {}", other),
                        });
                    }
                };
                Ok(FilterCondition::BooleanEquals {
                    field: "active".to_string(),
                    value: flag,
                })
            }
            other => Err(RepoError::InvalidRequest {
                message: format!("Unknown filter field: {}", other),
            }),
        }
    }

    fn default_sorts() -> [SortField; 2] {
        [
            SortField {
                name: "created_at",
                path: "created_at_ts",
                default_order: SortOrder::Desc,
            },
            SortField {
                name: "member_count",
                path: "member_count",
                default_order: SortOrder::Asc,
            },
        ]
    }

    #[test]
    fn into_params_applies_defaults_and_parses_filters() {
        let query = SearchQuery {
            page: None,
            page_size: None,
            sort_by: Some("created_at".to_string()),
            sort_order: None,
            q: None,
            filter: vec![
                "visibility:eq:public".to_string(),
                "member_count:range:10,50".to_string(),
            ],
        };

        let sorts = default_sorts();
        let params = query
            .into_params(&sorts, &sorts[0], mock_filter_mapper)
            .expect("query should parse");

        assert_eq!(params.page, 1);
        assert_eq!(params.page_size, 25);

        let sort = params.sort.expect("sort should be present");
        assert_eq!(sort.field, "created_at_ts");
        assert_eq!(sort.order, SortOrder::Desc);

        assert_eq!(params.conditions.len(), 2);
        assert_eq!(params.conditions[0].to_query_clause(), "(@visibility:{public})");
        assert_eq!(params.conditions[1].to_query_clause(), "(@member_count:[10 50])");
    }

    #[test]
    fn into_params_caps_page_size_and_overrides_sort_order() {
        let query = SearchQuery {
            page: Some(2),
            page_size: Some(500),
            sort_by: Some("member_count".to_string()),
            sort_order: Some(SortOrder::Asc),
            q: None,
            filter: Vec::new(),
        };

        let sorts = default_sorts();
        let params = query
            .into_params(&sorts, &sorts[0], mock_filter_mapper)
            .expect("query should parse");

        assert_eq!(params.page, 2);
        assert_eq!(params.page_size, 100);
        let sort = params.sort.expect("sort should be present");
        assert_eq!(sort.field, "member_count");
        assert_eq!(sort.order, SortOrder::Asc);
    }

    #[test]
    fn into_params_rejects_unknown_sort_field() {
        let query = SearchQuery {
            page: None,
            page_size: None,
            sort_by: Some("unknown".to_string()),
            sort_order: None,
            q: None,
            filter: Vec::new(),
        };

        let err = query
            .into_params(&default_sorts(), &default_sorts()[0], mock_filter_mapper)
            .expect_err("unknown sort should fail");
        assert!(matches!(err, RepoError::InvalidRequest { message } if message.contains("Unsupported sort field")));
    }

    #[test]
    fn into_params_rejects_invalid_filter_syntax() {
        let query = SearchQuery {
            page: None,
            page_size: None,
            sort_by: None,
            sort_order: None,
            q: None,
            filter: vec!["visibility".to_string()],
        };

        let err = query
            .into_params(&default_sorts(), &default_sorts()[0], mock_filter_mapper)
            .expect_err("invalid filter syntax should fail");
        assert!(matches!(err, RepoError::InvalidRequest { message } if message.contains("Invalid filter syntax")));
    }

    #[test]
    fn into_params_rejects_unsupported_operator() {
        let query = SearchQuery {
            page: None,
            page_size: None,
            sort_by: None,
            sort_order: None,
            q: None,
            filter: vec!["member_count:gt:10".to_string()],
        };

        let err = query
            .into_params(&default_sorts(), &default_sorts()[0], mock_filter_mapper)
            .expect_err("unsupported operator should fail");
        assert!(
            matches!(err, RepoError::InvalidRequest { message } if message.contains("Unsupported filter operator"))
        );
    }

    #[test]
    fn into_params_parses_boolean_filters() {
        let query = SearchQuery {
            page: None,
            page_size: None,
            sort_by: None,
            sort_order: None,
            q: None,
            filter: vec!["active:bool:true".to_string()],
        };

        let params = query
            .into_params(&default_sorts(), &default_sorts()[0], mock_filter_mapper)
            .expect("bool filter should parse");

        assert_eq!(params.conditions.len(), 1);
        assert_eq!(params.conditions[0].to_query_clause(), "(@active:{true})");
    }

    #[test]
    fn with_text_query_attaches_full_text_clause() {
        let query = SearchQuery {
            page: None,
            page_size: None,
            sort_by: None,
            sort_order: None,
            q: Some("dragon knights".to_string()),
            filter: vec!["visibility:eq:public".to_string()],
        };

        let sorts = default_sorts();
        let params = query
            .with_text_query(&sorts, &sorts[0], mock_filter_mapper, &["name", "description"])
            .expect("text query should parse");

        assert!(params.text_query.is_some());
        assert_eq!(params.conditions.len(), 1);
        assert_eq!(params.conditions[0].to_query_clause(), "(@visibility:{public})");
    }

    #[test]
    fn boolean_filter_rejects_invalid_value() {
        let query = SearchQuery {
            page: None,
            page_size: None,
            sort_by: None,
            sort_order: None,
            q: None,
            filter: vec!["active:bool:notabool".to_string()],
        };

        let err = query
            .into_params(&default_sorts(), &default_sorts()[0], mock_filter_mapper)
            .expect_err("invalid bool should fail");
        assert!(matches!(err, RepoError::InvalidRequest { message } if message.contains("Invalid boolean value")));
    }

    #[test]
    fn escape_for_tag_query_escapes_required_characters() {
        // Per RediSearch docs: $ { } \ | need escaping in TAG fields
        // Additionally, - must be escaped as it's the NOT operator in query syntax
        let original = "guild name|with spaces";
        let escaped = escape_for_tag_query(original);
        assert_eq!(escaped, "guild name\\|with spaces");
    }

    #[test]
    fn escape_for_tag_query_handles_dollar_sign() {
        assert_eq!(escape_for_tag_query("$100"), "\\$100");
        assert_eq!(escape_for_tag_query("price$"), "price\\$");
    }

    #[test]
    fn escape_for_tag_query_handles_hyphen() {
        // Hyphen must be escaped as it's the NOT operator in query syntax
        assert_eq!(escape_for_tag_query("test-device"), "test\\-device");
        assert_eq!(escape_for_tag_query("a-b-c"), "a\\-b\\-c");
    }

    #[test]
    fn escape_for_tag_query_allows_spaces_and_other_punctuation() {
        // Spaces, colons, brackets, quotes are allowed in TAG fields
        assert_eq!(escape_for_tag_query("New York"), "New York");
        assert_eq!(escape_for_tag_query("key:value"), "key:value");
        assert_eq!(escape_for_tag_query("[tag]"), "[tag]");
        assert_eq!(escape_for_tag_query("it's"), "it's");
        // Periods ARE escaped (JSON path separator)
        assert_eq!(escape_for_tag_query("v1.0.0"), "v1\\.0\\.0");
        assert_eq!(escape_for_tag_query("list.test"), "list\\.test");
    }

    #[test]
    fn escape_for_tag_query_handles_all_special_chars() {
        assert_eq!(escape_for_tag_query("${foo|bar}"), "\\$\\{foo\\|bar\\}");
        assert_eq!(escape_for_tag_query("a\\b"), "a\\\\b");
        assert_eq!(escape_for_tag_query("test-user$1"), "test\\-user\\$1");
    }

    #[test]
    fn build_text_query_generates_expected_expression() {
        let query = build_text_query(Some("dragon riders".to_string()), &["name", "description"]).unwrap();
        assert!(query.contains("@name:(dragon* riders*)"));
        assert!(query.contains("@description:(dragon* riders*)"));
    }

    #[test]
    fn range_filter_query() {
        let condition = FilterCondition::NumericRange {
            field: "created_at".to_string(),
            min: Some(100.0),
            max: None,
        };

        assert_eq!(condition.to_query_clause(), "(@created_at:[100 +inf])");
    }

    // TEXT field filter tests

    #[test]
    fn text_prefix_filter_tokenizes_on_slash() {
        // Slashes are tokenizers - path is split into separate terms
        let condition = FilterCondition::TextPrefix {
            field: "path".to_string(),
            value: "config/db".to_string(),
        };
        // Tokenized into "config" and "db", wildcard on last token
        assert_eq!(condition.to_query_clause(), "(@path:config db*)");
    }

    #[test]
    fn text_prefix_filter_tokenizes_on_dash() {
        // Dashes are tokenizers in TEXT fields AND parsed as negation in queries
        // We must tokenize ourselves to avoid '-' being interpreted as NOT
        let condition = FilterCondition::TextPrefix {
            field: "path".to_string(),
            value: "cli-kv-tests/abc/list".to_string(),
        };
        // Tokenized into "cli", "kv", "tests", "abc", "list*"
        assert_eq!(condition.to_query_clause(), "(@path:cli kv tests abc list*)");
    }

    #[test]
    fn text_prefix_filter_escapes_special_chars_in_tokens() {
        // Special chars in tokens are escaped, but value is still tokenized
        let condition = FilterCondition::TextPrefix {
            field: "path".to_string(),
            value: "user:name@domain".to_string(),
        };
        // No tokenizers, so single token with escaping
        assert_eq!(condition.to_query_clause(), "(@path:user\\:name\\@domain*)");
    }

    #[test]
    fn text_prefix_filter_handles_trailing_slash() {
        // Trailing slash is split out and filtered as empty, last real token gets wildcard
        let condition = FilterCondition::TextPrefix {
            field: "path".to_string(),
            value: "config/db/".to_string(),
        };
        assert_eq!(condition.to_query_clause(), "(@path:config db*)");
    }

    #[test]
    fn text_prefix_filter_handles_consecutive_dashes() {
        // Consecutive dashes produce empty strings that should be filtered out
        let condition = FilterCondition::TextPrefix {
            field: "path".to_string(),
            value: "a--b---c".to_string(),
        };
        assert_eq!(condition.to_query_clause(), "(@path:a b c*)");
    }

    #[test]
    fn text_prefix_filter_handles_leading_trailing_dashes() {
        // Leading and trailing dashes produce empty strings that should be filtered out
        let condition = FilterCondition::TextPrefix {
            field: "path".to_string(),
            value: "-foo-bar-".to_string(),
        };
        assert_eq!(condition.to_query_clause(), "(@path:foo bar*)");
    }

    #[test]
    fn text_prefix_filter_handles_mixed_separators() {
        // Mixed dashes and slashes should all be treated as separators
        let condition = FilterCondition::TextPrefix {
            field: "path".to_string(),
            value: "a-b/c--d/e".to_string(),
        };
        assert_eq!(condition.to_query_clause(), "(@path:a b c d e*)");
    }

    #[test]
    fn text_contains_filter_query() {
        let condition = FilterCondition::TextContains {
            field: "description".to_string(),
            value: "important".to_string(),
        };
        assert_eq!(condition.to_query_clause(), "(@description:*important*)");
    }

    #[test]
    fn text_contains_filter_escapes_special_chars() {
        let condition = FilterCondition::TextContains {
            field: "path".to_string(),
            value: "test/path".to_string(),
        };
        assert_eq!(condition.to_query_clause(), "(@path:*test/path*)");
    }

    #[test]
    fn text_exact_filter_query() {
        let condition = FilterCondition::TextExact {
            field: "name".to_string(),
            value: "exact phrase".to_string(),
        };
        assert_eq!(condition.to_query_clause(), "(@name:\"exact phrase\")");
    }

    #[test]
    fn text_exact_filter_escapes_quotes() {
        let condition = FilterCondition::TextExact {
            field: "name".to_string(),
            value: "phrase with \"quotes\"".to_string(),
        };
        assert_eq!(condition.to_query_clause(), "(@name:\"phrase with \\\"quotes\\\"\")");
    }

    #[test]
    fn text_fuzzy_filter_query() {
        let condition = FilterCondition::TextFuzzy {
            field: "name".to_string(),
            value: "hello".to_string(),
        };
        assert_eq!(condition.to_query_clause(), "(@name:%hello%)");
    }

    #[test]
    fn text_fuzzy_filter_escapes_special_chars() {
        let condition = FilterCondition::TextFuzzy {
            field: "name".to_string(),
            value: "hello%world".to_string(),
        };
        assert_eq!(condition.to_query_clause(), "(@name:%hello\\%world%)");
    }

    // Tests for the new public escaping API

    #[test]
    fn escape_for_text_prefix_simple() {
        assert_eq!(escape_for_text_prefix("config"), "config*");
        assert_eq!(escape_for_text_prefix("hello"), "hello*");
    }

    #[test]
    fn escape_for_text_prefix_tokenizes_path() {
        assert_eq!(escape_for_text_prefix("cli-kv/data"), "cli kv data*");
        assert_eq!(escape_for_text_prefix("config/db-settings"), "config db settings*");
    }

    #[test]
    fn escape_for_text_prefix_handles_special_chars() {
        assert_eq!(escape_for_text_prefix("user:name"), "user\\:name*");
        assert_eq!(escape_for_text_prefix("test@example"), "test\\@example*");
    }

    #[test]
    fn escape_for_text_contains_wraps_with_wildcards() {
        assert_eq!(escape_for_text_contains("hello"), "*hello*");
        assert_eq!(escape_for_text_contains("error"), "*error*");
    }

    #[test]
    fn escape_for_text_contains_escapes_special_chars() {
        assert_eq!(escape_for_text_contains("name@domain"), "*name\\@domain*");
        assert_eq!(escape_for_text_contains("50%"), "*50\\%*");
    }

    #[test]
    fn escape_for_text_exact_wraps_with_quotes() {
        assert_eq!(escape_for_text_exact("hello world"), "\"hello world\"");
        assert_eq!(escape_for_text_exact("John Doe"), "\"John Doe\"");
    }

    #[test]
    fn escape_for_text_exact_escapes_quotes() {
        assert_eq!(escape_for_text_exact("say \"hello\""), "\"say \\\"hello\\\"\"");
    }

    #[test]
    fn escape_for_text_fuzzy_wraps_with_percent() {
        assert_eq!(escape_for_text_fuzzy("hello"), "%hello%");
        assert_eq!(escape_for_text_fuzzy("wrold"), "%wrold%");
    }

    #[test]
    fn escape_for_text_fuzzy_escapes_special_chars() {
        assert_eq!(escape_for_text_fuzzy("test%value"), "%test\\%value%");
    }

    #[test]
    fn escape_for_text_search_adds_wildcard() {
        assert_eq!(escape_for_text_search("dragon"), "dragon*");
        assert_eq!(escape_for_text_search("hello"), "hello*");
    }

    #[test]
    fn escape_for_text_search_escapes_special_chars() {
        assert_eq!(escape_for_text_search("user:test"), "user\\:test*");
    }

    #[test]
    fn into_params_parses_prefix_filters() {
        fn text_filter_mapper(descriptor: FilterDescriptor) -> Result<FilterCondition, RepoError> {
            match descriptor.field.as_str() {
                "path" => crate::filters::normalizers::build_text_filter(descriptor, "path"),
                other => Err(RepoError::InvalidRequest {
                    message: format!("Unknown filter field: {}", other),
                }),
            }
        }

        let query = SearchQuery {
            page: None,
            page_size: None,
            sort_by: None,
            sort_order: None,
            q: None,
            filter: vec!["path:prefix:config/db".to_string()],
        };

        let params = query
            .into_params(&default_sorts(), &default_sorts()[0], text_filter_mapper)
            .expect("prefix filter should parse");

        assert_eq!(params.conditions.len(), 1);
        // Path is tokenized on '/' into ["config", "db"], query has wildcard on last token
        assert_eq!(params.conditions[0].to_query_clause(), "(@path:config db*)");
    }

    #[test]
    fn into_params_parses_contains_filters() {
        fn text_filter_mapper(descriptor: FilterDescriptor) -> Result<FilterCondition, RepoError> {
            match descriptor.field.as_str() {
                "description" => crate::filters::normalizers::build_text_filter(descriptor, "description"),
                other => Err(RepoError::InvalidRequest {
                    message: format!("Unknown filter field: {}", other),
                }),
            }
        }

        let query = SearchQuery {
            page: None,
            page_size: None,
            sort_by: None,
            sort_order: None,
            q: None,
            filter: vec!["description:contains:important".to_string()],
        };

        let params = query
            .into_params(&default_sorts(), &default_sorts()[0], text_filter_mapper)
            .expect("contains filter should parse");

        assert_eq!(params.conditions.len(), 1);
        assert_eq!(params.conditions[0].to_query_clause(), "(@description:*important*)");
    }

    #[test]
    fn into_params_parses_exact_filters() {
        fn text_filter_mapper(descriptor: FilterDescriptor) -> Result<FilterCondition, RepoError> {
            match descriptor.field.as_str() {
                "name" => crate::filters::normalizers::build_text_filter(descriptor, "name"),
                other => Err(RepoError::InvalidRequest {
                    message: format!("Unknown filter field: {}", other),
                }),
            }
        }

        let query = SearchQuery {
            page: None,
            page_size: None,
            sort_by: None,
            sort_order: None,
            q: None,
            filter: vec!["name:exact:exact phrase".to_string()],
        };

        let params = query
            .into_params(&default_sorts(), &default_sorts()[0], text_filter_mapper)
            .expect("exact filter should parse");

        assert_eq!(params.conditions.len(), 1);
        assert_eq!(params.conditions[0].to_query_clause(), "(@name:\"exact phrase\")");
    }

    #[test]
    fn into_params_parses_fuzzy_filters() {
        fn text_filter_mapper(descriptor: FilterDescriptor) -> Result<FilterCondition, RepoError> {
            match descriptor.field.as_str() {
                "name" => crate::filters::normalizers::build_text_filter(descriptor, "name"),
                other => Err(RepoError::InvalidRequest {
                    message: format!("Unknown filter field: {}", other),
                }),
            }
        }

        let query = SearchQuery {
            page: None,
            page_size: None,
            sort_by: None,
            sort_order: None,
            q: None,
            filter: vec!["name:fuzzy:hello".to_string()],
        };

        let params = query
            .into_params(&default_sorts(), &default_sorts()[0], text_filter_mapper)
            .expect("fuzzy filter should parse");

        assert_eq!(params.conditions.len(), 1);
        assert_eq!(params.conditions[0].to_query_clause(), "(@name:%hello%)");
    }

    #[test]
    fn text_eq_defaults_to_prefix() {
        fn text_filter_mapper(descriptor: FilterDescriptor) -> Result<FilterCondition, RepoError> {
            match descriptor.field.as_str() {
                "path" => crate::filters::normalizers::build_text_filter(descriptor, "path"),
                other => Err(RepoError::InvalidRequest {
                    message: format!("Unknown filter field: {}", other),
                }),
            }
        }

        let query = SearchQuery {
            page: None,
            page_size: None,
            sort_by: None,
            sort_order: None,
            q: None,
            filter: vec!["path:eq:config".to_string()],
        };

        let params = query
            .into_params(&default_sorts(), &default_sorts()[0], text_filter_mapper)
            .expect("eq filter should parse");

        assert_eq!(params.conditions.len(), 1);
        // Eq on TEXT fields defaults to prefix for backwards compatibility
        assert_eq!(params.conditions[0].to_query_clause(), "(@path:config*)");
    }

    // ==========================================================================
    // And/Or Composition Tests
    // ==========================================================================

    #[test]
    fn or_with_two_conditions() {
        let condition = FilterCondition::or([
            FilterCondition::bool_eq("private", false),
            FilterCondition::tag_eq("owner", "user123"),
        ]);

        assert_eq!(condition.to_query_clause(), "((@private:{false})|(@owner:{user123}))");
    }

    #[test]
    fn or_with_three_conditions() {
        let condition = FilterCondition::or([
            FilterCondition::tag_eq("status", "active"),
            FilterCondition::tag_eq("status", "pending"),
            FilterCondition::tag_eq("status", "review"),
        ]);

        assert_eq!(
            condition.to_query_clause(),
            "((@status:{active})|(@status:{pending})|(@status:{review}))"
        );
    }

    #[test]
    fn and_with_two_conditions() {
        let condition = FilterCondition::and([
            FilterCondition::tag_eq("status", "active"),
            FilterCondition::bool_eq("verified", true),
        ]);

        assert_eq!(condition.to_query_clause(), "((@status:{active}) (@verified:{true}))");
    }

    #[test]
    fn and_with_three_conditions() {
        let condition = FilterCondition::and([
            FilterCondition::tag_eq("type", "article"),
            FilterCondition::bool_eq("published", true),
            FilterCondition::tag_eq("category", "tech"),
        ]);

        assert_eq!(
            condition.to_query_clause(),
            "((@type:{article}) (@published:{true}) (@category:{tech}))"
        );
    }

    #[test]
    fn nested_or_within_and() {
        // (status = active OR status = pending) AND verified = true
        let condition = FilterCondition::and([
            FilterCondition::or([
                FilterCondition::tag_eq("status", "active"),
                FilterCondition::tag_eq("status", "pending"),
            ]),
            FilterCondition::bool_eq("verified", true),
        ]);

        assert_eq!(
            condition.to_query_clause(),
            "(((@status:{active})|(@status:{pending})) (@verified:{true}))"
        );
    }

    #[test]
    fn nested_and_within_or() {
        // (status = active AND verified = true) OR (status = pending AND priority = high)
        let condition = FilterCondition::or([
            FilterCondition::and([
                FilterCondition::tag_eq("status", "active"),
                FilterCondition::bool_eq("verified", true),
            ]),
            FilterCondition::and([
                FilterCondition::tag_eq("status", "pending"),
                FilterCondition::tag_eq("priority", "high"),
            ]),
        ]);

        assert_eq!(
            condition.to_query_clause(),
            "(((@status:{active}) (@verified:{true}))|((@status:{pending}) (@priority:{high})))"
        );
    }

    #[test]
    fn deeply_nested_conditions() {
        // ((A OR B) AND C) OR ((D AND E) OR F)
        let condition = FilterCondition::or([
            FilterCondition::and([
                FilterCondition::or([FilterCondition::tag_eq("a", "1"), FilterCondition::tag_eq("b", "2")]),
                FilterCondition::tag_eq("c", "3"),
            ]),
            FilterCondition::or([
                FilterCondition::and([FilterCondition::tag_eq("d", "4"), FilterCondition::tag_eq("e", "5")]),
                FilterCondition::tag_eq("f", "6"),
            ]),
        ]);

        assert_eq!(
            condition.to_query_clause(),
            "((((@a:{1})|(@b:{2})) (@c:{3}))|(((@d:{4}) (@e:{5}))|(@f:{6})))"
        );
    }

    #[test]
    fn or_with_single_condition_simplifies() {
        let condition = FilterCondition::or([FilterCondition::tag_eq("status", "active")]);

        // Single item Or should just return the inner condition
        assert_eq!(condition.to_query_clause(), "(@status:{active})");
    }

    #[test]
    fn and_with_single_condition_simplifies() {
        let condition = FilterCondition::and([FilterCondition::bool_eq("verified", true)]);

        // Single item And should just return the inner condition
        assert_eq!(condition.to_query_clause(), "(@verified:{true})");
    }

    #[test]
    fn or_empty_returns_empty() {
        let condition = FilterCondition::Or(vec![]);

        assert_eq!(condition.to_query_clause(), "");
    }

    #[test]
    fn and_empty_returns_empty() {
        let condition = FilterCondition::And(vec![]);

        assert_eq!(condition.to_query_clause(), "");
    }

    #[test]
    fn visibility_pattern_matches_kv_usage() {
        // This is the exact pattern used in KV manager for visibility filtering
        let visibility = FilterCondition::or([
            FilterCondition::bool_eq("private", false),
            FilterCondition::tag_eq("owner", "test-user"),
        ]);

        assert_eq!(visibility.to_query_clause(), "((@private:{false})|(@owner:{test\\-user}))");
    }

    #[test]
    fn mixed_types_in_and() {
        // Combine TAG, BOOL, and NUMERIC in an AND
        let condition = FilterCondition::and([
            FilterCondition::tag_eq("category", "electronics"),
            FilterCondition::bool_eq("in_stock", true),
            FilterCondition::NumericRange {
                field: "price".to_string(),
                min: Some(100.0),
                max: Some(500.0),
            },
        ]);

        assert_eq!(
            condition.to_query_clause(),
            "((@category:{electronics}) (@in_stock:{true}) (@price:[100 500]))"
        );
    }

    #[test]
    fn mixed_types_in_or() {
        // Combine different types in an OR (less common but should work)
        let condition = FilterCondition::or([
            FilterCondition::tag_eq("status", "featured"),
            FilterCondition::NumericRange {
                field: "rating".to_string(),
                min: Some(4.5),
                max: None,
            },
        ]);

        assert_eq!(condition.to_query_clause(), "((@status:{featured})|(@rating:[4.5 +inf]))");
    }

    #[test]
    fn tag_eq_builder_single_value() {
        let condition = FilterCondition::tag_eq("status", "active");

        assert_eq!(condition.to_query_clause(), "(@status:{active})");
    }

    #[test]
    fn tag_eq_builder_escapes_special_chars() {
        let condition = FilterCondition::tag_eq("owner", "user-123");

        assert_eq!(condition.to_query_clause(), "(@owner:{user\\-123})");
    }

    #[test]
    fn bool_eq_builder_true() {
        let condition = FilterCondition::bool_eq("active", true);

        assert_eq!(condition.to_query_clause(), "(@active:{true})");
    }

    #[test]
    fn bool_eq_builder_false() {
        let condition = FilterCondition::bool_eq("deleted", false);

        assert_eq!(condition.to_query_clause(), "(@deleted:{false})");
    }

    #[test]
    fn search_params_with_multiple_conditions_anded() {
        // SearchParams ANDs all top-level conditions
        let params = SearchParams::new()
            .with_condition(FilterCondition::tag_eq("status", "active"))
            .with_condition(FilterCondition::bool_eq("verified", true));

        let query = params.build_query("");

        // Both conditions should appear, separated by space (implicit AND)
        assert!(query.contains("(@status:{active})"));
        assert!(query.contains("(@verified:{true})"));
    }

    #[test]
    fn search_params_with_or_condition() {
        let params = SearchParams::new().with_condition(FilterCondition::or([
            FilterCondition::bool_eq("private", false),
            FilterCondition::tag_eq("owner", "me"),
        ]));

        let query = params.build_query("");

        assert_eq!(query, "((@private:{false})|(@owner:{me}))");
    }

    #[test]
    fn search_params_with_and_and_or_combined() {
        // Visibility OR + other filters (how KV manager uses it)
        let visibility = FilterCondition::or([
            FilterCondition::bool_eq("private", false),
            FilterCondition::tag_eq("owner", "user1"),
        ]);

        let params = SearchParams::new()
            .with_condition(visibility)
            .with_condition(FilterCondition::tag_eq("indexed", "true"));

        let query = params.build_query("");

        // Should have visibility OR and indexed filter, space-separated (ANDed)
        assert!(query.contains("((@private:{false})|(@owner:{user1}))"));
        assert!(query.contains("(@indexed:{true})"));
    }

    #[test]
    fn search_params_build_query_with_base() {
        let params = SearchParams::new().with_condition(FilterCondition::tag_eq("status", "active"));

        let query = params.build_query("@tenant:{acme}");

        assert!(query.starts_with("(@tenant:{acme})"));
        assert!(query.contains("(@status:{active})"));
    }

    #[test]
    fn complex_real_world_query() {
        // A realistic complex query:
        // (private=false OR owner=currentUser) AND status=active AND (type=article OR type=post)
        let visibility = FilterCondition::or([
            FilterCondition::bool_eq("private", false),
            FilterCondition::tag_eq("owner", "user123"),
        ]);

        let type_filter = FilterCondition::or([
            FilterCondition::tag_eq("type", "article"),
            FilterCondition::tag_eq("type", "post"),
        ]);

        let params = SearchParams::new()
            .with_condition(visibility)
            .with_condition(FilterCondition::tag_eq("status", "active"))
            .with_condition(type_filter);

        let query = params.build_query("");

        // All three top-level conditions should be present
        assert!(query.contains("((@private:{false})|(@owner:{user123}))"));
        assert!(query.contains("(@status:{active})"));
        assert!(query.contains("((@type:{article})|(@type:{post}))"));
    }

    // ==========================================================================
    // Raw Query Escape Hatch Tests
    // ==========================================================================

    #[test]
    fn raw_query_alone() {
        // User provides their own expert RediSearch query
        let params = SearchParams::new().with_raw("@custom_field:{special_value}");

        let query = params.build_query("");

        assert_eq!(query, "(@custom_field:{special_value})");
    }

    #[test]
    fn raw_query_with_conditions() {
        // Raw query combined with structured conditions
        let params = SearchParams::new()
            .with_condition(FilterCondition::tag_eq("status", "active"))
            .with_raw("@geo:[lon lat 10 km]");

        let query = params.build_query("");

        // Both should be present, raw comes after conditions
        assert!(query.contains("(@status:{active})"));
        assert!(query.contains("(@geo:[lon lat 10 km])"));
    }

    #[test]
    fn raw_query_with_base() {
        // Raw query with tenant base filter
        let params = SearchParams::new().with_raw("@special:custom_syntax");

        let query = params.build_query("@tenant:{acme}");

        assert!(query.starts_with("(@tenant:{acme})"));
        assert!(query.contains("(@special:custom_syntax)"));
    }

    #[test]
    fn raw_query_complex_redisearch_syntax() {
        // Test that complex RediSearch syntax passes through unchanged
        let complex_query = "@title:(hello|world) @price:[100 500] -@status:{deleted}";
        let params = SearchParams::new().with_raw(complex_query);

        let query = params.build_query("");

        assert_eq!(query, format!("({})", complex_query));
    }

    #[test]
    fn raw_query_with_or_and_conditions() {
        // Combine structured Or condition with raw escape hatch
        let visibility = FilterCondition::or([
            FilterCondition::bool_eq("private", false),
            FilterCondition::tag_eq("owner", "user1"),
        ]);

        let params = SearchParams::new()
            .with_condition(visibility)
            .with_raw("@location:[lon lat radius km]");

        let query = params.build_query("");

        assert!(query.contains("((@private:{false})|(@owner:{user1}))"));
        assert!(query.contains("(@location:[lon lat radius km])"));
    }

    #[test]
    fn raw_query_empty_string_ignored() {
        // Empty raw query should not add anything
        let params = SearchParams::new()
            .with_condition(FilterCondition::tag_eq("status", "active"))
            .with_raw("");

        let query = params.build_query("");

        assert_eq!(query, "(@status:{active})");
    }

    #[test]
    fn raw_query_with_text_query() {
        // Both text_query and raw can be used together
        let params = SearchParams::new()
            .with_text_query("(@name:(dragon* knight*))|(@desc:(dragon* knight*))")
            .with_raw("@score:[90 +inf]");

        let query = params.build_query("");

        assert!(query.contains("(@name:(dragon* knight*))|(@desc:(dragon* knight*))"));
        assert!(query.contains("(@score:[90 +inf])"));
    }

    #[test]
    fn raw_query_geo_radius_example() {
        // Real-world example: geo radius search
        let params = SearchParams::new()
            .with_condition(FilterCondition::tag_eq("type", "restaurant"))
            .with_condition(FilterCondition::bool_eq("open", true))
            .with_raw("@location:[-122.4194 37.7749 5 km]");

        let query = params.build_query("");

        assert!(query.contains("(@type:{restaurant})"));
        assert!(query.contains("(@open:{true})"));
        assert!(query.contains("(@location:[-122.4194 37.7749 5 km])"));
    }

    #[test]
    fn raw_query_negation_example() {
        // Real-world example: negation in raw query
        let params = SearchParams::new()
            .with_condition(FilterCondition::tag_eq("category", "electronics"))
            .with_raw("-@brand:{excluded_brand}");

        let query = params.build_query("");

        assert!(query.contains("(@category:{electronics})"));
        assert!(query.contains("(-@brand:{excluded_brand})"));
    }

    #[test]
    fn raw_query_vector_search_example() {
        // Real-world example: vector similarity search
        let params = SearchParams::new()
            .with_condition(FilterCondition::bool_eq("indexed", true))
            .with_raw("@embedding:[VECTOR_RANGE 0.5 $vec]");

        let query = params.build_query("");

        assert!(query.contains("(@indexed:{true})"));
        assert!(query.contains("(@embedding:[VECTOR_RANGE 0.5 $vec])"));
    }

    #[test]
    fn raw_query_order_is_last() {
        // Verify raw query comes after all other clauses
        let params = SearchParams::new()
            .with_condition(FilterCondition::tag_eq("a", "1"))
            .with_text_query("search terms")
            .with_raw("@raw_field:value");

        let query = params.build_query("@base:filter");

        // Order should be: base, conditions, text_query, raw
        let base_pos = query.find("(@base:filter)").unwrap();
        let condition_pos = query.find("(@a:{1})").unwrap();
        let text_pos = query.find("(search terms)").unwrap();
        let raw_pos = query.find("(@raw_field:value)").unwrap();

        assert!(base_pos < condition_pos);
        assert!(condition_pos < text_pos);
        assert!(text_pos < raw_pos);
    }
}
