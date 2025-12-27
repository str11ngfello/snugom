pub(crate) struct ParsedField {
    ident: Ident,
    name: String,
    ty: TypeInfo,
    validations: Vec<FieldValidation>,
    datetime_mirror: Option<String>,
    is_id: bool,
    auto_updated: bool,
    auto_created: bool,
    // Search-related fields
    index_spec: Option<IndexSpec>,
    filter_spec: Option<FilterSpec>,
    is_searchable: bool,
    // Relation inference
    relation_spec: Option<FieldRelationSpec>,
}

/// Specification for a field-based relation
#[derive(Clone)]
pub(crate) struct FieldRelationSpec {
    /// The kind of relation (inferred or explicit)
    pub kind: RelationKind,
    /// Target collection name (inferred from field name or Vec<T> type)
    pub target: String,
    /// Alias for the relation (defaults to field name)
    pub alias: String,
    /// Cascade policy on delete
    pub cascade: CascadePolicy,
    /// Foreign key field (for belongs_to, this is the field itself)
    pub foreign_key: Option<String>,
    /// For many_to_many: the junction table name (reserved for future use)
    #[allow(dead_code)]
    pub junction: Option<String>,
}

/// Specification for how a field should be indexed in RediSearch
#[derive(Clone)]
pub(crate) struct IndexSpec {
    pub field_type: IndexFieldType,
    pub sortable: bool,
}

/// Specification for how a field should be exposed as an API filter
#[derive(Clone)]
pub(crate) struct FilterSpec {
    pub field_type: FilterFieldType,
    pub alias: Option<String>,
}

/// RediSearch index field types
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum IndexFieldType {
    Tag,
    Text,
    Numeric,
    Geo,
}

/// Filter field types for API mapping
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum FilterFieldType {
    Tag,
    Text,
    Numeric,
    Boolean,
    Geo,
}

#[derive(Clone)]
struct TypeInfo {
    optional: bool,
    base: FieldBase,
    element: Option<ElementType>,
    is_datetime: bool,
    ty: Type,
    option_inner: Option<Type>,
}

#[derive(Clone, Copy)]
enum FieldBase {
    String,
    Vec,
    Numeric,
    Boolean,
    Other,
}

#[derive(Clone)]
struct ElementType {
    #[allow(dead_code)]
    optional: bool,
    base: FieldBase,
    is_datetime: bool,
    #[allow(dead_code)]
    ty: Type,
    /// The type name for Vec<T> elements (e.g., "GuildMember" for Vec<GuildMember>)
    type_name: Option<String>,
}

struct FieldValidation {
    scope: ValidationScope,
    data: ValidationData,
}

#[derive(Clone, Copy)]
enum ValidationScope {
    Field,
    EachElement,
}

enum ValidationData {
    Length {
        min: Option<usize>,
        max: Option<usize>,
    },
    Range {
        min: Option<TokenStream2>,
        min_repr: Option<String>,
        max: Option<TokenStream2>,
        max_repr: Option<String>,
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
        expr: TokenStream2,
        expr_repr: String,
    },
    ForbiddenIf {
        expr: TokenStream2,
        expr_repr: String,
    },
    Unique {
        case_insensitive: bool,
    },
    Custom {
        path: TokenStream2,
        path_repr: String,
    },
}

#[derive(Clone, Copy)]
pub(crate) enum RelationKind {
    HasMany,
    ManyToMany,
    BelongsTo,
}

#[derive(Clone, Copy)]
pub(crate) enum CascadePolicy {
    Delete,
    Detach,
    None,
}
