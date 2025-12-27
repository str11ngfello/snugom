impl ParsedField {
    fn from_field(field: &Field) -> Result<Self> {
        let ident = field
            .ident
            .clone()
            .ok_or_else(|| Error::new(field.span(), "SnugomEntity requires named fields"))?;
        let name = ident.to_string();

        let ty = classify_type(&field.ty);
        let mut validations = Vec::new();
        let mut datetime_mirror = None;
        let mut is_id = false;
        let mut auto_updated = false;
        let mut auto_created = false;
        let mut index_spec = None;
        let mut filter_spec = None;
        let mut is_searchable = false;
        let mut relation_spec = None;

        for attr in &field.attrs {
            if attr.path().is_ident("snugom") {
                Self::parse_field_attr(
                    attr,
                    &ty,
                    &mut validations,
                    &mut datetime_mirror,
                    &mut is_id,
                    &mut auto_updated,
                    &mut auto_created,
                    &mut index_spec,
                    &mut filter_spec,
                    &mut is_searchable,
                    &mut relation_spec,
                    &name,
                )?;
            }
        }

        Ok(Self {
            ident,
            name,
            ty,
            validations,
            datetime_mirror,
            is_id,
            auto_updated,
            auto_created,
            index_spec,
            filter_spec,
            is_searchable,
            relation_spec,
        })
    }

    fn parse_field_attr(
        attr: &Attribute,
        ty: &TypeInfo,
        validations: &mut Vec<FieldValidation>,
        datetime_mirror: &mut Option<String>,
        is_id: &mut bool,
        auto_updated: &mut bool,
        auto_created: &mut bool,
        index_spec: &mut Option<IndexSpec>,
        filter_spec: &mut Option<FilterSpec>,
        is_searchable: &mut bool,
        relation_spec: &mut Option<FieldRelationSpec>,
        field_name: &str,
    ) -> Result<()> {
        // Track if we see sortable to apply after determining index type
        let mut saw_sortable = false;
        let mut filter_alias: Option<String> = None;

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("relation") {
                Self::parse_relation_attr(&meta, ty, relation_spec, field_name)?;
            } else if meta.path.is_ident("validate") {
                meta.parse_nested_meta(|rule| parse_validation_rule(rule, ty, validations, field_name))?;
            } else if meta.path.is_ident("datetime") {
                let mut has_epoch = false;
                meta.parse_nested_meta(|item| {
                    if item.path.is_ident("epoch_millis") {
                        has_epoch = true;
                    }
                    Ok(())
                })?;
                if has_epoch {
                    if !ty.is_datetime {
                        return Err(meta.error(
                            "#[snugom(datetime(...))] requires a chrono::DateTime<Tz> field or Option thereof",
                        ));
                    }
                    *datetime_mirror = Some(format!("{}_ts", field_name));
                }
            } else if meta.path.is_ident("id") {
                if *is_id {
                    return Err(meta.error("field already marked as #[snugom(id)]"));
                }
                if ty.optional {
                    return Err(meta.error("#[snugom(id)] cannot be applied to an Option<T>"));
                }
                if !matches!(ty.base, FieldBase::String) {
                    return Err(meta.error("#[snugom(id)] requires a field of type String"));
                }
                *is_id = true;
            } else if meta.path.is_ident("updated_at") {
                if *auto_updated {
                    return Err(meta.error("field already marked as #[snugom(updated_at)]"));
                }
                if !ty.is_datetime {
                    return Err(meta.error("#[snugom(updated_at)] requires a chrono::DateTime<Tz> field"));
                }
                *auto_updated = true;
            } else if meta.path.is_ident("created_at") {
                if *auto_created {
                    return Err(meta.error("field already marked as #[snugom(created_at)]"));
                }
                if !ty.is_datetime {
                    return Err(meta.error("#[snugom(created_at)] requires a chrono::DateTime<Tz> field"));
                }
                *auto_created = true;
            } else if meta.path.is_ident("sortable") {
                saw_sortable = true;
            } else if meta.path.is_ident("searchable") {
                // searchable only works on String types - full-text search doesn't apply to numbers or enums
                if !matches!(ty.base, FieldBase::String) {
                    return Err(meta.error("searchable can only be used on String fields; use filterable for numeric or enum types"));
                }
                // searchable implies TEXT index and is_searchable = true
                *is_searchable = true;
                let idx = index_spec.get_or_insert(IndexSpec {
                    field_type: IndexFieldType::Text,
                    sortable: false,
                });
                idx.field_type = IndexFieldType::Text;
            } else if meta.path.is_ident("filterable") {
                // Parse optional type: filterable or filterable(tag) or filterable(text) etc.
                let filter_type = Self::parse_filter_type(&meta, ty)?;
                let index_type = Self::filter_to_index_type(filter_type);

                // Set index (filterable implies indexed)
                let idx = index_spec.get_or_insert(IndexSpec {
                    field_type: index_type,
                    sortable: false,
                });
                // Only override if not already set to a more specific type
                if idx.field_type != IndexFieldType::Text || filter_type == FilterFieldType::Text {
                    idx.field_type = index_type;
                }

                // Set filter
                *filter_spec = Some(FilterSpec {
                    field_type: filter_type,
                    alias: None, // alias parsed separately
                });
            } else if meta.path.is_ident("indexed") {
                // Parse optional type: indexed or indexed(tag) or indexed(text) etc.
                let index_type = Self::parse_index_type(&meta, ty)?;
                let idx = index_spec.get_or_insert(IndexSpec {
                    field_type: index_type,
                    sortable: false,
                });
                idx.field_type = index_type;
            } else if meta.path.is_ident("alias") {
                let value: LitStr = meta.value()?.parse()?;
                filter_alias = Some(value.value());
            } else if meta.path.is_ident("unique") {
                // Parse optional case_insensitive flag: unique or unique(case_insensitive)
                let mut case_insensitive = false;
                if meta.input.peek(syn::token::Paren) {
                    let content;
                    syn::parenthesized!(content in meta.input);
                    let inner: syn::Ident = content.parse()?;
                    if inner == "case_insensitive" {
                        case_insensitive = true;
                    } else {
                        return Err(syn::Error::new(
                            inner.span(),
                            format!("unknown unique option `{}`, expected `case_insensitive`", inner),
                        ));
                    }
                }
                validations.push(FieldValidation {
                    scope: ValidationScope::Field,
                    data: ValidationData::Unique { case_insensitive },
                });
            }
            Ok(())
        })?;

        // Check for incompatible combination: searchable (TEXT) + filterable(tag) (TAG)
        // These create a mismatch where the filter expects TAG semantics but the index is TEXT.
        // TEXT indexes tokenize on punctuation, breaking exact/prefix matching that TAG provides.
        if *is_searchable
            && let Some(fs) = filter_spec.as_ref()
            && fs.field_type == FilterFieldType::Tag
        {
            return Err(Error::new(
                attr.span(),
                "searchable and filterable(tag) cannot be used together on the same field; \
                 searchable creates a TEXT index (tokenized full-text search) while \
                 filterable(tag) expects a TAG index (exact/prefix matching). \
                 Choose one: use searchable for full-text search, or filterable(tag) for exact matching.",
            ));
        }

        // Apply sortable flag if we saw it
        if saw_sortable {
            if let Some(idx) = index_spec {
                idx.sortable = true;
            } else {
                // sortable without any index annotation - infer type
                let inferred = Self::infer_index_type(ty)
                    .ok_or_else(|| Error::new(attr.span(), "sortable on String requires searchable or filterable(tag/text) to determine index type"))?;
                *index_spec = Some(IndexSpec {
                    field_type: inferred,
                    sortable: true,
                });
            }
        }

        // Apply alias to filter spec if present
        if let Some(alias) = filter_alias
            && let Some(fs) = filter_spec
        {
            fs.alias = Some(alias);
        }

        Ok(())
    }

    /// Parse the `#[snugom(relation)]` or `#[snugom(relation(...))]` attribute
    ///
    /// Inference rules:
    /// - `#[snugom(relation)]` on `Vec<T>` → has_many to collection inferred from T
    /// - `#[snugom(relation)]` on `{entity}_id: String` → belongs_to inferred from field name
    /// - `#[snugom(relation(many_to_many = "junction"))]` → explicit many_to_many
    /// - `#[snugom(relation(cascade = "delete"))]` → set cascade policy
    fn parse_relation_attr(
        meta: &syn::meta::ParseNestedMeta,
        ty: &TypeInfo,
        relation_spec: &mut Option<FieldRelationSpec>,
        field_name: &str,
    ) -> Result<()> {
        if relation_spec.is_some() {
            return Err(meta.error("field already has a relation attribute"));
        }

        // Default cascade policy
        let mut cascade = CascadePolicy::None;
        let mut explicit_target: Option<String> = None;
        let mut explicit_alias: Option<String> = None;
        let mut junction: Option<String> = None;
        let mut explicit_foreign_key: Option<String> = None;

        // Parse optional nested attributes: relation(...) or just relation
        if meta.input.peek(syn::token::Paren) {
            meta.parse_nested_meta(|nested| {
                if nested.path.is_ident("cascade") {
                    let value: LitStr = nested.value()?.parse()?;
                    cascade = match value.value().as_str() {
                        "delete" => CascadePolicy::Delete,
                        "detach" => CascadePolicy::Detach,
                        "none" => CascadePolicy::None,
                        other => return Err(nested.error(format!("unknown cascade policy `{other}`, expected delete, detach, or none"))),
                    };
                } else if nested.path.is_ident("many_to_many") {
                    let value: LitStr = nested.value()?.parse()?;
                    junction = Some(value.value());
                } else if nested.path.is_ident("target") {
                    let value: LitStr = nested.value()?.parse()?;
                    explicit_target = Some(value.value());
                } else if nested.path.is_ident("alias") {
                    let value: LitStr = nested.value()?.parse()?;
                    explicit_alias = Some(value.value());
                } else if nested.path.is_ident("foreign_key") {
                    let value: LitStr = nested.value()?.parse()?;
                    explicit_foreign_key = Some(value.value());
                } else {
                    return Err(nested.error("unknown relation attribute, expected cascade, many_to_many, target, alias, or foreign_key"));
                }
                Ok(())
            })?;
        }

        // Infer relation kind and target based on field type and name
        let (kind, target, alias, foreign_key) = if let Some(ref junction_target) = junction {
            // Explicit many_to_many - must be Vec<T>
            // The junction value IS the target collection
            if !matches!(ty.base, FieldBase::Vec) {
                return Err(meta.error("many_to_many relation must be on a Vec<T> field"));
            }
            let inferred_target = explicit_target.unwrap_or_else(|| junction_target.clone());
            let inferred_alias = explicit_alias.unwrap_or_else(|| field_name.to_string());
            (RelationKind::ManyToMany, inferred_target, inferred_alias, junction.clone())
        } else if matches!(ty.base, FieldBase::Vec) {
            // Vec<T> → has_many
            let element_type = ty.element.as_ref()
                .and_then(|e| e.type_name.clone())
                .ok_or_else(|| meta.error("cannot infer target type for has_many relation; ensure Vec contains a named type"))?;
            let inferred_target = explicit_target.unwrap_or_else(|| to_snake_plural(&element_type));
            let inferred_alias = explicit_alias.unwrap_or_else(|| field_name.to_string());
            (RelationKind::HasMany, inferred_target, inferred_alias, None)
        } else if matches!(ty.base, FieldBase::String) && field_name.ends_with("_id") {
            // {entity}_id: String → belongs_to
            let entity_prefix = &field_name[..field_name.len() - 3]; // Remove "_id"
            let inferred_target = explicit_target.unwrap_or_else(|| format!("{entity_prefix}s")); // Simple pluralization
            let inferred_alias = explicit_alias.unwrap_or_else(|| entity_prefix.to_string());
            let fk = explicit_foreign_key.unwrap_or_else(|| field_name.to_string());
            (RelationKind::BelongsTo, inferred_target, inferred_alias, Some(fk))
        } else {
            return Err(meta.error(
                "cannot infer relation type; use #[snugom(relation)] on Vec<T> for has_many, \
                 on {entity}_id: String for belongs_to, or specify many_to_many explicitly"
            ));
        };

        *relation_spec = Some(FieldRelationSpec {
            kind,
            target,
            alias,
            cascade,
            foreign_key,
            junction,
        });

        Ok(())
    }

    /// Parse filterable type: filterable or filterable(tag) or filterable(text) etc.
    fn parse_filter_type(meta: &syn::meta::ParseNestedMeta, ty: &TypeInfo) -> Result<FilterFieldType> {
        // Check if there are parentheses with a type
        if meta.input.peek(syn::token::Paren) {
            let content;
            parenthesized!(content in meta.input);
            let type_ident: Ident = content.parse()?;
            match type_ident.to_string().as_str() {
                "tag" => Ok(FilterFieldType::Tag),
                "text" => {
                    // filterable(text) only makes sense on String types
                    if !matches!(ty.base, FieldBase::String) {
                        return Err(Error::new(type_ident.span(), "filterable(text) can only be used on String fields; numeric types are always NUMERIC"));
                    }
                    Ok(FilterFieldType::Text)
                }
                "numeric" => Ok(FilterFieldType::Numeric),
                "boolean" | "bool" => Ok(FilterFieldType::Boolean),
                "geo" => {
                    // filterable(geo) requires String type for "lat,lon" format
                    if !matches!(ty.base, FieldBase::String) {
                        return Err(Error::new(type_ident.span(), "filterable(geo) can only be used on String fields (\"lat,lon\" format); use filterable for numeric types"));
                    }
                    Ok(FilterFieldType::Geo)
                }
                other => Err(Error::new(type_ident.span(), format!("unknown filter type '{}', expected tag, text, numeric, boolean, or geo", other))),
            }
        } else {
            // No explicit type - infer from Rust type
            Self::infer_filter_type(ty)
                .ok_or_else(|| meta.error("filterable on String requires explicit type: filterable(tag) or filterable(text)"))
        }
    }

    /// Parse indexed type: indexed or indexed(tag) or indexed(text) etc.
    fn parse_index_type(meta: &syn::meta::ParseNestedMeta, ty: &TypeInfo) -> Result<IndexFieldType> {
        // Check if there are parentheses with a type
        if meta.input.peek(syn::token::Paren) {
            let content;
            parenthesized!(content in meta.input);
            let type_ident: Ident = content.parse()?;
            match type_ident.to_string().as_str() {
                "tag" => Ok(IndexFieldType::Tag),
                "text" => Ok(IndexFieldType::Text),
                "numeric" => Ok(IndexFieldType::Numeric),
                "geo" => Ok(IndexFieldType::Geo),
                other => Err(Error::new(type_ident.span(), format!("unknown index type '{}', expected tag, text, numeric, or geo", other))),
            }
        } else {
            // No explicit type - infer from Rust type
            Self::infer_index_type(ty)
                .ok_or_else(|| meta.error("indexed on String requires explicit type: indexed(tag) or indexed(text)"))
        }
    }

    /// Infer index type from Rust type
    fn infer_index_type(ty: &TypeInfo) -> Option<IndexFieldType> {
        if ty.is_datetime {
            return Some(IndexFieldType::Numeric);
        }
        match ty.base {
            FieldBase::Numeric => Some(IndexFieldType::Numeric),
            FieldBase::Boolean => Some(IndexFieldType::Tag),
            FieldBase::Vec => Some(IndexFieldType::Tag), // Vec<String> = array of tags
            FieldBase::String => None, // Ambiguous: could be TEXT or TAG
            FieldBase::Other => Some(IndexFieldType::Tag), // Assume enum -> TAG
        }
    }

    /// Infer filter type from Rust type
    fn infer_filter_type(ty: &TypeInfo) -> Option<FilterFieldType> {
        if ty.is_datetime {
            return Some(FilterFieldType::Numeric);
        }
        match ty.base {
            FieldBase::Numeric => Some(FilterFieldType::Numeric),
            FieldBase::Boolean => Some(FilterFieldType::Boolean),
            FieldBase::Vec => Some(FilterFieldType::Tag), // Vec<String> = array of tags
            FieldBase::String => None, // Ambiguous: could be TEXT or TAG
            FieldBase::Other => Some(FilterFieldType::Tag), // Assume enum -> TAG
        }
    }

    /// Convert filter type to corresponding index type
    fn filter_to_index_type(filter_type: FilterFieldType) -> IndexFieldType {
        match filter_type {
            FilterFieldType::Tag => IndexFieldType::Tag,
            FilterFieldType::Text => IndexFieldType::Text,
            FilterFieldType::Numeric => IndexFieldType::Numeric,
            FilterFieldType::Boolean => IndexFieldType::Tag, // booleans stored as TAG
            FilterFieldType::Geo => IndexFieldType::Geo,
        }
    }

    fn to_descriptor_tokens(&self) -> TokenStream2 {
        let name = &self.name;
        let optional = self.ty.optional;
        let is_id = self.is_id;
        let auto_updated = self.auto_updated;
        let auto_created = self.auto_created;
        let datetime_mirror = match &self.datetime_mirror {
            Some(value) => {
                let lit = LitStr::new(value, Span::call_site());
                quote! { Some(#lit.to_string()) }
            }
            None => quote! { None },
        };
        let validations = self.validations.iter().map(|validation| validation.to_descriptor_tokens());
        let field_type = self.field_type_tokens();
        let element_type = self.element_type_tokens();

        // Relation Vec fields (has_many, many_to_many) default to empty and skip required checks
        let is_relation_vec = self.relation_spec.is_some() && matches!(self.ty.base, FieldBase::Vec);

        // For non-primitive types (FieldBase::Other) that are filterable as TAG,
        // we need to normalize enum values at write time. Enums with associated data
        // serialize to objects (e.g., {"swiss": {"rounds": 6}}), which RediSearch
        // cannot index as TAG fields. Setting this flag tells the repository to
        // extract just the variant name (discriminant) for the indexed value.
        let normalize_enum_tag = self.needs_enum_tag_normalization();

        quote! {
            ::snugom::types::FieldDescriptor {
                name: #name.to_string(),
                optional: #optional,
                is_id: #is_id,
                validations: vec![#(#validations),*],
                datetime_mirror: #datetime_mirror,
                auto_updated: #auto_updated,
                auto_created: #auto_created,
                field_type: #field_type,
                element_type: #element_type,
                is_relation_vec: #is_relation_vec,
                normalize_enum_tag: #normalize_enum_tag,
            }
        }
    }

    /// Returns true if this field needs enum tag normalization for RediSearch indexing.
    /// This is needed for non-primitive types (enums) that are filterable as TAG,
    /// since enums with associated data serialize to objects rather than strings.
    fn needs_enum_tag_normalization(&self) -> bool {
        // Must be a non-primitive type (likely an enum)
        if !matches!(self.ty.base, FieldBase::Other) {
            return false;
        }
        // Must be filterable as TAG
        match &self.filter_spec {
            Some(spec) => spec.field_type == FilterFieldType::Tag,
            None => false,
        }
    }

    fn validation_snippets(&self, field_idents: &[Ident]) -> Vec<TokenStream2> {
        let mut snippets = Vec::new();
        for validation in &self.validations {
            match validation.scope {
                ValidationScope::Field => {
                    snippets.push(validation.emit_field_check(self, field_idents));
                }
                ValidationScope::EachElement => {
                    snippets.push(validation.emit_each_check(self));
                }
            }
        }
        snippets
    }

    fn datetime_mirror_snippet(&self) -> Option<TokenStream2> {
        let mirror = self.datetime_mirror.as_ref()?;
        let field_lit = LitStr::new(&self.name, Span::call_site());
        let mirror_lit = LitStr::new(mirror, Span::call_site());
        let field_ident = &self.ident;
        if self.ty.optional {
            Some(quote! {
                mirrors.push(::snugom::types::DatetimeMirrorValue::new(
                    #field_lit,
                    #mirror_lit,
                    self.#field_ident.as_ref().map(|value| value.timestamp_millis()),
                ));
            })
        } else {
            Some(quote! {
                mirrors.push(::snugom::types::DatetimeMirrorValue::new(
                    #field_lit,
                    #mirror_lit,
                    Some(self.#field_ident.timestamp_millis()),
                ));
            })
        }
    }

    fn builder_field_definition(&self) -> TokenStream2 {
        let ident = &self.ident;
        let storage_ty = if self.ty.optional {
            let inner = self.ty.option_inner.as_ref().expect("optional field must have inner type");
            quote! { Option<Option<#inner>> }
        } else {
            let ty = &self.ty.ty;
            quote! { Option<#ty> }
        };
        quote! { #ident: #storage_ty }
    }

    fn builder_setter_methods(&self) -> TokenStream2 {
        let ident = &self.ident;
        let setter = format_ident!("set_{}", ident);
        let field_lit = LitStr::new(&self.name, Span::call_site());
        let record_override = if self.auto_updated {
            Some(quote! {
                self.managed_overrides.insert(#field_lit.to_string());
            })
        } else {
            None
        };
        if self.ty.optional {
            let inner = self.ty.option_inner.as_ref().expect("optional field must have inner type");
            if matches!(self.ty.base, FieldBase::String) {
                quote! {
                    pub fn #ident<S>(mut self, value: Option<S>) -> Self
                    where
                        S: ::std::convert::Into<String>,
                    {
                        self.#ident = Some(value.map(|inner| inner.into()));
                        #record_override
                        self
                    }

                    pub fn #setter<S>(&mut self, value: Option<S>) -> &mut Self
                    where
                        S: ::std::convert::Into<String>,
                    {
                        self.#ident = Some(value.map(|inner| inner.into()));
                        #record_override
                        self
                    }
                }
            } else {
                quote! {
                    pub fn #ident(mut self, value: Option<#inner>) -> Self {
                        self.#ident = Some(value);
                        #record_override
                        self
                    }

                    pub fn #setter(&mut self, value: Option<#inner>) -> &mut Self {
                        self.#ident = Some(value);
                        #record_override
                        self
                    }
                }
            }
        } else {
            let ty = &self.ty.ty;
            if matches!(self.ty.base, FieldBase::String) {
                quote! {
                    pub fn #ident<S>(mut self, value: S) -> Self
                    where
                        S: ::std::convert::Into<String>,
                    {
                        self.#ident = Some(value.into());
                        #record_override
                        self
                    }

                    pub fn #setter<S>(&mut self, value: S) -> &mut Self
                    where
                        S: ::std::convert::Into<String>,
                    {
                        self.#ident = Some(value.into());
                        #record_override
                        self
                    }
                }
            } else {
                quote! {
                    pub fn #ident(mut self, value: #ty) -> Self {
                        self.#ident = Some(value);
                        #record_override
                        self
                    }

                    pub fn #setter(&mut self, value: #ty) -> &mut Self {
                        self.#ident = Some(value);
                        #record_override
                        self
                    }
                }
            }
        }
    }

    fn builder_required_check(&self) -> Option<TokenStream2> {
        if self.is_id {
            return None;
        }
        if self.ty.optional {
            return None;
        }
        if self.auto_updated || self.auto_created {
            return None;
        }
        // Relation Vec fields (for has_many, many_to_many) are for hydration and default to empty
        if self.relation_spec.is_some() && matches!(self.ty.base, FieldBase::Vec) {
            return None;
        }
        let ident = &self.ident;
        let field_lit = LitStr::new(&self.name, Span::call_site());
        Some(quote! {
            if self.#ident.is_none() {
                issues.push(::snugom::errors::ValidationIssue::new(
                    #field_lit,
                    "validation.required",
                    "field is required",
                ));
            }
        })
    }

    fn builder_value_binding(&self, allow_missing: bool) -> TokenStream2 {
        let ident = &self.ident;
        if self.ty.optional {
            quote! {
                let #ident = self.#ident.take().unwrap_or(None);
            }
        } else if self.auto_updated || self.auto_created {
            quote! {
                let #ident = self.#ident.take().unwrap_or_else(|| ::chrono::Utc::now());
            }
        } else if allow_missing && matches!(self.ty.base, FieldBase::String) {
            quote! {
                let #ident = self
                    .#ident
                    .take()
                    .unwrap_or_else(|| "__snugom_pending_fk__".to_string());
            }
        } else if self.relation_spec.is_some() && matches!(self.ty.base, FieldBase::Vec) {
            // Relation Vec fields default to empty Vec
            quote! {
                let #ident = self.#ident.take().unwrap_or_else(Vec::new);
            }
        } else {
            quote! {
                let #ident = self.#ident.take().unwrap();
            }
        }
    }

    fn patch_setter_method(&self) -> TokenStream2 {
        let ident = &self.ident;
        let field_lit = LitStr::new(&self.name, Span::call_site());
        let path_lit = LitStr::new(&format!("$.{}", self.name), Span::call_site());
        let serialization_error = quote! {
            self.validation_issues.push(::snugom::errors::ValidationIssue::new(
                #field_lit,
                "serialization.failed",
                err.to_string(),
            ));
        };

        let is_string = matches!(self.ty.base, FieldBase::String);

        if self.ty.optional {
            if is_string {
                quote! {
                    pub fn #ident<S>(mut self, value: Option<S>) -> Self
                    where
                        S: ::std::convert::Into<String>,
                    {
                        match value {
                            Some(inner) => {
                                let owned = inner.into();
                                self.operations.push(::snugom::repository::PatchOperation {
                                    path: #path_lit.to_string(),
                                    kind: ::snugom::repository::PatchOpKind::Assign(::serde_json::Value::String(owned)),
                                    mirror: ::std::option::Option::None,
                                });
                            }
                            None => {
                                self.operations.push(::snugom::repository::PatchOperation {
                                    path: #path_lit.to_string(),
                                    kind: ::snugom::repository::PatchOpKind::Delete,
                                    mirror: ::std::option::Option::None,
                                });
                            }
                        }
                        self
                    }
                }
            } else {
                let inner = self.ty.option_inner.as_ref().expect("optional field must have inner type");
                let mirror_assign = if let Some(mirror) = &self.datetime_mirror {
                    let mirror_lit = LitStr::new(mirror, Span::call_site());
                    quote! {
                        let mirror = ::std::option::Option::Some(::snugom::types::DatetimeMirrorValue::new(
                            #field_lit,
                            #mirror_lit,
                            ::std::option::Option::Some(inner.timestamp_millis()),
                        ));
                    }
                } else {
                    quote! { let mirror = ::std::option::Option::None; }
                };
                let mirror_delete = if let Some(mirror) = &self.datetime_mirror {
                    let mirror_lit = LitStr::new(mirror, Span::call_site());
                    quote! {
                        let mirror = ::std::option::Option::Some(::snugom::types::DatetimeMirrorValue::new(
                            #field_lit,
                            #mirror_lit,
                            ::std::option::Option::None,
                        ));
                    }
                } else {
                    quote! { let mirror = ::std::option::Option::None; }
                };

                quote! {
                    pub fn #ident(mut self, value: Option<#inner>) -> Self {
                        match value {
                            Some(inner) => {
                                match ::serde_json::to_value(&inner) {
                                    Ok(json_value) => {
                                        #mirror_assign
                                        self.operations.push(::snugom::repository::PatchOperation {
                                            path: #path_lit.to_string(),
                                            kind: ::snugom::repository::PatchOpKind::Assign(json_value),
                                            mirror,
                                        });
                                    }
                                    Err(err) => {
                                        #serialization_error
                                    }
                                }
                            }
                            None => {
                                #mirror_delete
                                self.operations.push(::snugom::repository::PatchOperation {
                                    path: #path_lit.to_string(),
                                    kind: ::snugom::repository::PatchOpKind::Delete,
                                    mirror,
                                });
                            }
                        }
                        self
                    }
                }
            }
        } else if is_string {
            quote! {
                pub fn #ident<S>(mut self, value: S) -> Self
                where
                    S: ::std::convert::Into<String>,
                {
                    let owned = value.into();
                    self.operations.push(::snugom::repository::PatchOperation {
                        path: #path_lit.to_string(),
                        kind: ::snugom::repository::PatchOpKind::Assign(::serde_json::Value::String(owned)),
                        mirror: ::std::option::Option::None,
                    });
                    self
                }
            }
        } else {
            let ty = &self.ty.ty;
            let mirror_assign = if let Some(mirror) = &self.datetime_mirror {
                let mirror_lit = LitStr::new(mirror, Span::call_site());
                quote! {
                    let mirror = ::std::option::Option::Some(::snugom::types::DatetimeMirrorValue::new(
                        #field_lit,
                        #mirror_lit,
                        ::std::option::Option::Some(value.timestamp_millis()),
                    ));
                }
            } else {
                quote! { let mirror = ::std::option::Option::None; }
            };

            quote! {
                pub fn #ident(mut self, value: #ty) -> Self {
                    match ::serde_json::to_value(&value) {
                        Ok(json_value) => {
                            #mirror_assign
                            self.operations.push(::snugom::repository::PatchOperation {
                                path: #path_lit.to_string(),
                                kind: ::snugom::repository::PatchOpKind::Assign(json_value),
                                mirror,
                            });
                        }
                        Err(err) => {
                            #serialization_error
                        }
                    }
                    self
                }
            }
        }
    }

    fn field_type_tokens(&self) -> TokenStream2 {
        map_field_type(self.ty.base, self.ty.is_datetime)
    }

    fn element_type_tokens(&self) -> TokenStream2 {
        if let Some(element) = &self.ty.element {
            let tokens = map_field_type(element.base, element.is_datetime);
            quote! { Some(#tokens) }
        } else {
            quote! { None }
        }
    }

    // ========== Search-related methods ==========

    /// Returns true if this field has an index specification
    pub(crate) fn has_index(&self) -> bool {
        self.index_spec.is_some()
    }

    /// Returns true if this field is included in full-text search
    pub(crate) fn is_text_searchable(&self) -> bool {
        self.is_searchable
    }

    /// Get the index field name (uses datetime mirror if applicable)
    pub(crate) fn index_field_name(&self) -> String {
        self.datetime_mirror.clone().unwrap_or_else(|| self.name.clone())
    }

    /// Get the filter alias or the field name
    pub(crate) fn filter_name(&self) -> String {
        self.filter_spec
            .as_ref()
            .and_then(|fs| fs.alias.clone())
            .unwrap_or_else(|| self.name.clone())
    }

    /// Generate the IndexField tokens for this field
    pub(crate) fn to_index_field_tokens(&self) -> Option<TokenStream2> {
        let idx = self.index_spec.as_ref()?;
        // For fields needing enum tag normalization, index the shadow field instead
        let (path, field_name) = if self.needs_enum_tag_normalization() {
            let shadow_name = format!("__{}_tag", self.name);
            (format!("$.{}", shadow_name), shadow_name)
        } else {
            (format!("$.{}", self.index_field_name()), self.index_field_name())
        };
        let field_type = match idx.field_type {
            IndexFieldType::Tag => quote! { ::snugom::search::IndexFieldType::Tag },
            IndexFieldType::Text => quote! { ::snugom::search::IndexFieldType::Text },
            IndexFieldType::Numeric => quote! { ::snugom::search::IndexFieldType::Numeric },
            IndexFieldType::Geo => quote! { ::snugom::search::IndexFieldType::Geo },
        };
        let sortable = idx.sortable;

        Some(quote! {
            ::snugom::search::IndexField {
                path: #path,
                field_name: #field_name,
                field_type: #field_type,
                sortable: #sortable,
            }
        })
    }

    /// Generate the SortField tokens for this field (if sortable)
    pub(crate) fn to_sort_field_tokens(&self) -> Option<TokenStream2> {
        let idx = self.index_spec.as_ref()?;
        if !idx.sortable {
            return None;
        }
        let name = &self.name;
        let path = self.index_field_name();
        let default_order = match idx.field_type {
            IndexFieldType::Numeric => quote! { ::snugom::search::SortOrder::Desc },
            _ => quote! { ::snugom::search::SortOrder::Asc },
        };

        Some(quote! {
            ::snugom::search::SortField {
                name: #name,
                path: #path,
                default_order: #default_order,
            }
        })
    }

    /// Generate the filter match arm for this field
    pub(crate) fn to_filter_match_arm(&self) -> Option<TokenStream2> {
        let fs = self.filter_spec.as_ref()?;
        let filter_name = self.filter_name();
        // For fields needing enum tag normalization, query the shadow field instead
        let query_field = if self.needs_enum_tag_normalization() {
            format!("__{}_tag", self.name)
        } else {
            self.index_field_name()
        };

        let arm = match fs.field_type {
            FilterFieldType::Tag => quote! {
                #filter_name => {
                    if descriptor.operator != ::snugom::search::FilterOperator::Eq {
                        return Err(::snugom::errors::RepoError::InvalidRequest {
                            message: format!("{} filter only supports eq operator", #filter_name),
                        });
                    }
                    if descriptor.values.is_empty() {
                        return Err(::snugom::errors::RepoError::InvalidRequest {
                            message: format!("{} filter requires at least one value", #filter_name),
                        });
                    }
                    Ok(::snugom::search::FilterCondition::TagEquals {
                        field: #query_field.to_string(),
                        values: descriptor.values,
                    })
                }
            },
            FilterFieldType::Numeric => quote! {
                #filter_name => {
                    ::snugom::filters::normalizers::build_numeric_filter(descriptor, #query_field)
                }
            },
            FilterFieldType::Text => quote! {
                #filter_name => {
                    ::snugom::filters::normalizers::build_text_filter(descriptor, #query_field)
                }
            },
            FilterFieldType::Boolean => quote! {
                #filter_name => {
                    if descriptor.operator != ::snugom::search::FilterOperator::Eq {
                        return Err(::snugom::errors::RepoError::InvalidRequest {
                            message: format!("{} filter only supports eq operator", #filter_name),
                        });
                    }
                    let value = descriptor.values.get(0).ok_or_else(|| {
                        ::snugom::errors::RepoError::InvalidRequest {
                            message: format!("{} filter requires a value", #filter_name),
                        }
                    })?;
                    let bool_value = match value.trim() {
                        "true" => true,
                        "false" => false,
                        _ => return Err(::snugom::errors::RepoError::InvalidRequest {
                            message: format!("Invalid boolean value for {}: {}", #filter_name, value),
                        }),
                    };
                    Ok(::snugom::search::FilterCondition::BooleanEquals {
                        field: #query_field.to_string(),
                        value: bool_value,
                    })
                }
            },
            FilterFieldType::Geo => quote! {
                #filter_name => {
                    Err(::snugom::errors::RepoError::InvalidRequest {
                        message: format!("Geo filter for {} not yet implemented", #filter_name),
                    })
                }
            },
        };

        Some(arm)
    }

    /// Returns the unique constraint info if this field has a #[snugom(unique)] validation
    pub(crate) fn unique_constraint_info(&self) -> Option<(String, bool)> {
        for validation in &self.validations {
            if let ValidationData::Unique { case_insensitive } = &validation.data {
                return Some((self.name.clone(), *case_insensitive));
            }
        }
        None
    }
}

fn map_field_type(base: FieldBase, is_datetime: bool) -> TokenStream2 {
    if is_datetime {
        return quote! { ::snugom::types::FieldType::DateTime };
    }
    match base {
        FieldBase::String => quote! { ::snugom::types::FieldType::String },
        FieldBase::Vec => quote! { ::snugom::types::FieldType::Array },
        FieldBase::Numeric => quote! { ::snugom::types::FieldType::Number },
        FieldBase::Boolean => quote! { ::snugom::types::FieldType::Boolean },
        FieldBase::Other => quote! { ::snugom::types::FieldType::Object },
    }
}
