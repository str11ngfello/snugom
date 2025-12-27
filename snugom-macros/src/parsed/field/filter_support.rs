pub(crate) struct FieldInfo {
    pub(crate) name: String,
    pub(crate) field_type: FilterFieldType,
    pub(crate) normalizer: Option<String>,
    pub(crate) aliases: Vec<String>,
}

// FilterFieldType is now defined in defs.rs
