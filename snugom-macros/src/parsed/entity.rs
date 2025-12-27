#[allow(unused_imports)]
use super::*;

pub(crate) struct ParsedEntity {
    name: Ident,
    version: u32,
    vis: Visibility,
    id_field: Ident,
    relations: Vec<ParsedRelation>,
    fields: Vec<ParsedField>,
    derived_id: Option<DerivedIdSpec>,
    // Search-related
    default_sort: Option<DefaultSortSpec>,
    // Unique constraints from #[snugom(unique_together = [...])]
    unique_together: Vec<UniqueTogetherSpec>,
}

/// Specification for entity-level compound unique constraint
struct UniqueTogetherSpec {
    fields: Vec<String>,
    case_insensitive: bool,
}

/// Specification for default sort order
pub(crate) struct DefaultSortSpec {
    pub field: String,
    pub descending: bool,
}

pub(crate) struct ParsedRelation {
    alias: String,
    target: String,
    kind: RelationKind,
    cascade: CascadePolicy,
    foreign_key: Option<String>,
}

struct DerivedIdSpec {
    components: Vec<String>,
    separator: String,
}

impl ParsedEntity {
    pub(crate) fn from_input(input: &DeriveInput) -> Result<Self> {
        let mut version = 1u32;
        let mut relations = Vec::new();
        let mut default_sort: Option<DefaultSortSpec> = None;
        let mut unique_together: Vec<UniqueTogetherSpec> = Vec::new();

        for attr in &input.attrs {
            if attr.path().is_ident("snugom") {
                Self::parse_container_attr(attr, &mut version, &mut relations, &mut default_sort, &mut unique_together)?;
            }
        }

        // service and collection come from bundle! macro via BundleRegistered trait

        let fields = match &input.data {
            Data::Struct(data) => match &data.fields {
                Fields::Named(named) => {
                    let mut parsed = Vec::new();
                    for field in &named.named {
                        parsed.push(ParsedField::from_field(field)?);
                    }
                    parsed
                }
                _ => return Err(Error::new(input.ident.span(), "SnugomEntity requires named fields")),
            },
            _ => return Err(Error::new(input.ident.span(), "SnugomEntity can only be derived for structs")),
        };

        let mut id_field_ident: Option<Ident> = None;
        for field in &fields {
            if field.is_id {
                if id_field_ident.is_some() {
                    return Err(Error::new(
                        field.ident.span(),
                        "SnugomEntity allows exactly one #[snugom(id)] field",
                    ));
                }
                id_field_ident = Some(field.ident.clone());
            }
        }

        let id_field = id_field_ident.ok_or_else(|| {
            Error::new(input.ident.span(), "SnugomEntity requires a field annotated with #[snugom(id)]")
        })?;

        // Collect field-based relations and merge with container-level relations
        let field_relations = Self::collect_field_relations(&fields);
        relations.extend(field_relations);

        let derived_id = Self::detect_derived_id(&fields, &relations);

        Ok(Self {
            name: input.ident.clone(),
            version,
            vis: input.vis.clone(),
            id_field,
            relations,
            fields,
            derived_id,
            default_sort,
            unique_together,
        })
    }

    /// Collect relations declared on fields via #[snugom(relation)]
    fn collect_field_relations(fields: &[ParsedField]) -> Vec<ParsedRelation> {
        fields
            .iter()
            .filter_map(|field| {
                field.relation_spec.as_ref().map(|spec| ParsedRelation {
                    alias: spec.alias.clone(),
                    target: spec.target.clone(),
                    kind: spec.kind,
                    cascade: spec.cascade,
                    foreign_key: spec.foreign_key.clone(),
                })
            })
            .collect()
    }

    #[allow(clippy::ptr_arg)]
    fn parse_container_attr(
        attr: &Attribute,
        version: &mut u32,
        _relations: &mut Vec<ParsedRelation>,
        default_sort: &mut Option<DefaultSortSpec>,
        unique_together: &mut Vec<UniqueTogetherSpec>,
    ) -> Result<()> {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("service") {
                return Err(meta.error("service is set via bundle! macro, not on the derive"));
            } else if meta.path.is_ident("collection") {
                return Err(meta.error("collection is set via bundle! macro, not on the derive"));
            } else if meta.path.is_ident("relationship") {
                return Err(meta.error(
                    "relationship() is no longer supported; use #[snugom(relation)] on fields instead"
                ));
            } else if meta.path.is_ident("version") {
                let value: LitInt = meta.value()?.parse()?;
                *version = value.base10_parse()?;
            } else if meta.path.is_ident("default_sort") {
                let value: LitStr = meta.value()?.parse()?;
                let raw = value.value();
                let (field, descending) = if let Some(stripped) = raw.strip_prefix('-') {
                    (stripped.to_string(), true)
                } else {
                    (raw, false)
                };
                *default_sort = Some(DefaultSortSpec { field, descending });
            } else if meta.path.is_ident("unique_together") {
                // Parse #[snugom(unique_together = ["field1", "field2"])]
                // or #[snugom(unique_together(case_insensitive) = ["field1", "field2"])]
                let mut case_insensitive = false;

                // Check for optional (case_insensitive) modifier
                if meta.input.peek(syn::token::Paren) {
                    let content;
                    syn::parenthesized!(content in meta.input);
                    let modifier: syn::Ident = content.parse()?;
                    if modifier == "case_insensitive" {
                        case_insensitive = true;
                    } else {
                        return Err(syn::Error::new(
                            modifier.span(),
                            format!("unknown unique_together option `{}`, expected `case_insensitive`", modifier),
                        ));
                    }
                }

                // Parse = ["field1", "field2"]
                meta.input.parse::<syn::Token![=]>()?;
                let content;
                syn::bracketed!(content in meta.input);
                let parsed: syn::punctuated::Punctuated<LitStr, syn::Token![,]> =
                    content.parse_terminated(<LitStr as Parse>::parse, syn::Token![,])?;
                let fields: Vec<String> = parsed.into_iter().map(|lit| lit.value()).collect();

                if fields.len() < 2 {
                    return Err(syn::Error::new(
                        attr.bracket_token.span.join(),
                        "unique_together requires at least 2 fields",
                    ));
                }

                unique_together.push(UniqueTogetherSpec { fields, case_insensitive });
            }
            Ok(())
        })
    }

    pub(crate) fn emit(&self) -> TokenStream2 {
        let name = &self.name;
        let version = self.version;
        let id_ident = &self.id_field;
        let id_field_lit = LitStr::new(&self.id_field.to_string(), Span::call_site());

        let relation_inits = self.relations.iter().map(|rel| rel.to_tokens());
        // Collect relation targets for compile-time validation
        let relation_targets: Vec<&str> = self.relations.iter().map(|rel| rel.target.as_str()).collect();
        let field_inits = self.fields.iter().map(|field| field.to_descriptor_tokens());
        let field_idents: Vec<Ident> = self.fields.iter().map(|field| field.ident.clone()).collect();

        // Collect unique constraints from field-level #[snugom(unique)] and entity-level #[snugom(unique_together)]
        let mut unique_constraint_tokens: Vec<TokenStream2> = Vec::new();

        // Field-level unique constraints (single-field)
        for field in &self.fields {
            if let Some((field_name, case_insensitive)) = field.unique_constraint_info() {
                unique_constraint_tokens.push(quote! {
                    ::snugom::types::UniqueConstraintDescriptor::single(#field_name, #case_insensitive)
                });
            }
        }

        // Entity-level compound unique constraints
        for spec in &self.unique_together {
            let field_names: Vec<_> = spec.fields.iter().map(|f| {
                let lit = LitStr::new(f, Span::call_site());
                quote! { #lit.to_string() }
            }).collect();
            let case_insensitive = spec.case_insensitive;
            unique_constraint_tokens.push(quote! {
                ::snugom::types::UniqueConstraintDescriptor::compound(
                    vec![#(#field_names),*],
                    #case_insensitive,
                )
            });
        }
        let validation_snippets: Vec<_> = self
            .fields
            .iter()
            .flat_map(|field| field.validation_snippets(&field_idents))
            .collect();
        let datetime_snippets: Vec<_> =
            self.fields.iter().filter_map(|field| field.datetime_mirror_snippet()).collect();
        let builder_ident = Ident::new(&format!("{}ValidationBuilder", name), Span::call_site());
        let patch_builder_ident = Ident::new(&format!("{}PatchBuilder", name), Span::call_site());
        let vis = &self.vis;
        let mut builder_fields: Vec<_> = self.fields.iter().map(|field| field.builder_field_definition()).collect();
        builder_fields.push(quote! { managed_overrides: ::std::collections::BTreeSet<::std::string::String> });
        builder_fields.push(quote! { relations: ::std::vec::Vec<::snugom::repository::RelationPlan> });
        builder_fields.push(quote! { nested_creates: ::std::vec::Vec<::snugom::repository::NestedMutation> });
        builder_fields.push(quote! { validation_issues: ::std::vec::Vec<::snugom::errors::ValidationIssue> });
        builder_fields.push(quote! { idempotency_key: ::std::option::Option<::std::string::String> });
        builder_fields.push(quote! { idempotency_ttl: ::std::option::Option<u64> });

        let mut builder_setters: Vec<_> = self.fields.iter().map(|field| field.builder_setter_methods()).collect();
        let patch_setters: Vec<_> = self.fields.iter().map(|field| field.patch_setter_method()).collect();
        let relation_methods = quote! {
            pub fn relation(
                mut self,
                alias: impl Into<String>,
                add: Vec<String>,
                remove: Vec<String>,
            ) -> Self {
                self.relations.push(::snugom::repository::RelationPlan::new(alias, add, remove));
                self
            }

            pub fn relation_mut(
                &mut self,
                alias: impl Into<String>,
                add: Vec<String>,
                remove: Vec<String>,
            ) -> &mut Self {
                self.relations.push(::snugom::repository::RelationPlan::new(alias, add, remove));
                self
            }

            pub fn connect(mut self, alias: impl Into<String>, values: Vec<String>) -> Self {
                self.relations
                    .push(::snugom::repository::RelationPlan::new(alias, values, Vec::new()));
                self
            }

            pub fn connect_mut(
                &mut self,
                alias: impl Into<String>,
                values: Vec<String>,
            ) -> &mut Self {
                self.relations
                    .push(::snugom::repository::RelationPlan::new(alias, values, Vec::new()));
                self
            }

            pub fn disconnect(mut self, alias: impl Into<String>, values: Vec<String>) -> Self {
                self.relations
                    .push(::snugom::repository::RelationPlan::new(alias, Vec::new(), values));
                self
            }

            pub fn disconnect_mut(
                &mut self,
                alias: impl Into<String>,
                values: Vec<String>,
            ) -> &mut Self {
                self.relations
                    .push(::snugom::repository::RelationPlan::new(alias, Vec::new(), values));
                self
            }

            pub fn delete(mut self, alias: impl Into<String>, values: Vec<String>) -> Self {
                let mut plan = ::snugom::repository::RelationPlan::new(alias, Vec::new(), Vec::new());
                plan.delete = values;
                self.relations.push(plan);
                self
            }

            pub fn delete_mut(
                &mut self,
                alias: impl Into<String>,
                values: Vec<String>,
            ) -> &mut Self {
                let mut plan = ::snugom::repository::RelationPlan::new(alias, Vec::new(), Vec::new());
                plan.delete = values;
                self.relations.push(plan);
                self
            }

            pub fn create_relation<B>(
                mut self,
                alias: impl Into<String>,
                builder: B,
            ) -> Self
            where
                B: ::snugom::repository::MutationPayloadBuilder,
                <B as ::snugom::repository::MutationPayloadBuilder>::Entity:
                    ::snugom::types::EntityMetadata,
            {
                let alias_string = alias.into();
                match builder.into_payload() {
                    Ok(payload) => {
                        <B::Entity as ::snugom::types::EntityMetadata>::ensure_registered();
                        let entity_id = payload.entity_id.clone();
                        self.relations
                            .push(::snugom::repository::RelationPlan::new(
                                alias_string.clone(),
                                vec![entity_id],
                                Vec::new(),
                            ));
                        self.nested_creates.push(::snugom::repository::NestedMutation {
                            alias: alias_string,
                            descriptor: <B::Entity as ::snugom::types::EntityMetadata>::entity_descriptor(),
                            payload,
                        });
                    }
                    Err(err) => {
                        self.validation_issues.extend(err.issues.into_iter());
                    }
                }
                self
            }

            pub fn create_relation_mut<B>(
                &mut self,
                alias: impl Into<String>,
                builder: B,
            ) -> &mut Self
            where
                B: ::snugom::repository::MutationPayloadBuilder,
                <B as ::snugom::repository::MutationPayloadBuilder>::Entity:
                    ::snugom::types::EntityMetadata,
            {
                let alias_string = alias.into();
                match builder.into_payload() {
                    Ok(payload) => {
                        <B::Entity as ::snugom::types::EntityMetadata>::ensure_registered();
                        let entity_id = payload.entity_id.clone();
                        self.relations
                            .push(::snugom::repository::RelationPlan::new(
                                alias_string.clone(),
                                vec![entity_id],
                                Vec::new(),
                            ));
                        self.nested_creates.push(::snugom::repository::NestedMutation {
                            alias: alias_string,
                            descriptor: <B::Entity as ::snugom::types::EntityMetadata>::entity_descriptor(),
                            payload,
                        });
                    }
                    Err(err) => {
                        self.validation_issues.extend(err.issues.into_iter());
                    }
                }
                self
            }
        };
        builder_setters.push(relation_methods);
        let idempotency_methods = quote! {
            pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
                self.idempotency_key = Some(key.into());
                self
            }

            pub fn set_idempotency_key(&mut self, key: impl Into<String>) -> &mut Self {
                self.idempotency_key = Some(key.into());
                self
            }

            pub fn idempotency_ttl(mut self, ttl_seconds: u64) -> Self {
                self.idempotency_ttl = Some(ttl_seconds);
                self
            }

            pub fn set_idempotency_ttl(&mut self, ttl_seconds: u64) -> &mut Self {
                self.idempotency_ttl = Some(ttl_seconds);
                self
            }

            pub fn clear_idempotency(mut self) -> Self {
                self.idempotency_key = None;
                self.idempotency_ttl = None;
                self
            }

            pub fn clear_idempotency_mut(&mut self) -> &mut Self {
                self.idempotency_key = None;
                self.idempotency_ttl = None;
                self
            }
        };
        builder_setters.push(idempotency_methods);
        let foreign_key_names: Vec<String> = self
            .relations
            .iter()
            .filter_map(|relation| relation.foreign_key.clone())
            .collect();
        let builder_required_checks: Vec<_> = self
            .fields
            .iter()
            .filter_map(|field| {
                if foreign_key_names.iter().any(|name| name == &field.name) {
                    None
                } else {
                    field.builder_required_check()
                }
            })
            .collect();
        let builder_value_bindings: Vec<_> = self
            .fields
            .iter()
            .map(|field| {
                let allow_missing = foreign_key_names.iter().any(|name| name == &field.name);
                field.builder_value_binding(allow_missing)
            })
            .collect();
        let builder_field_names: Vec<_> = self.fields.iter().map(|field| field.ident.clone()).collect();
        let id_autofill = quote! {
            if self.#id_ident.is_none() {
                self.#id_ident = Some(::snugom::id::generate_entity_id());
            }
        };
        let datetime_method = {
            let body = if datetime_snippets.is_empty() {
                quote! { ::std::vec::Vec::new() }
            } else {
                quote! {
                    let mut mirrors = ::std::vec::Vec::new();
                    #(#datetime_snippets)*
                    mirrors
                }
            };
            quote! {
                pub fn datetime_mirrors(&self) -> ::snugom::types::DatetimeMirrors {
                    #body
                }
            }
        };

        let descriptor_static_ident = format_ident!("__SNUGOM_DESCRIPTOR_{}", self.name.to_string().to_uppercase());
        let register_static_ident = format_ident!("__SNUGOM_REGISTER_{}", self.name.to_string().to_uppercase());
        let derived_id_tokens = if let Some(spec) = &self.derived_id {
            let separator_lit = LitStr::new(&spec.separator, Span::call_site());
            let component_tokens: Vec<_> = spec
                .components
                .iter()
                .map(|component| {
                    let lit = LitStr::new(component, Span::call_site());
                    quote! { #lit.to_string() }
                })
                .collect();
            quote! {
                ::std::option::Option::Some(::snugom::types::DerivedIdDescriptor {
                    separator: #separator_lit.to_string(),
                    components: ::std::vec::Vec::from([#(#component_tokens),*]),
                })
            }
        } else {
            quote! { ::std::option::Option::None }
        };

        // Generate SearchEntity implementation if there are indexed fields
        let has_indexed_fields = self.fields.iter().any(|f| f.has_index());
        let search_entity_impl = self.emit_search_entity();

        quote! {
            #[allow(non_upper_case_globals)]
            static #descriptor_static_ident: ::std::sync::OnceLock<::snugom::types::EntityDescriptor> = ::std::sync::OnceLock::new();

            #[allow(non_upper_case_globals)]
            static #register_static_ident: ::std::sync::Once = ::std::sync::Once::new();

            impl ::snugom::types::EntityMetadata for #name {
                const HAS_INDEXED_FIELDS: bool = #has_indexed_fields;

                fn entity_descriptor() -> ::snugom::types::EntityDescriptor {
                    #register_static_ident.call_once(|| {
                        let descriptor = #descriptor_static_ident.get_or_init(|| ::snugom::types::EntityDescriptor {
                            service: <#name as ::snugom::types::BundleRegistered>::SERVICE.to_string(),
                            collection: <#name as ::snugom::types::BundleRegistered>::COLLECTION.to_string(),
                            version: #version,
                            id_field: Some(#id_field_lit.to_string()),
                            relations: vec![#(#relation_inits),*],
                            fields: vec![#(#field_inits),*],
                            derived_id: #derived_id_tokens,
                            unique_constraints: vec![#(#unique_constraint_tokens),*],
                        });
                        ::snugom::registry::register_descriptor(descriptor);
                    });
                    #descriptor_static_ident.get().expect("descriptor initialized above").clone()
                }

                fn ensure_registered() {
                    let _ = <#name as ::snugom::types::EntityMetadata>::entity_descriptor();
                }
            }

            impl #name {
                /// Relation targets for compile-time validation in bundle! macro
                pub const RELATION_TARGETS: &'static [&'static str] = &[#(#relation_targets),*];

                pub fn validate(&self) -> ::snugom::errors::ValidationResult<()> {
                    let mut issues: Vec<::snugom::errors::ValidationIssue> = Vec::new();
                    #(#validation_snippets)*
                    if issues.is_empty() {
                        Ok(())
                    } else {
                        Err(::snugom::errors::ValidationError::new(issues))
                    }
                }

                #datetime_method
            }

            #[derive(Debug, Clone, Default)]
            #vis struct #builder_ident {
                #(#builder_fields,)*
            }

            impl #builder_ident {
                pub fn new() -> Self {
                    Self::default()
                }

                #(#builder_setters)*

                pub fn build(mut self) -> ::snugom::errors::ValidationResult<#name> {
                    #id_autofill
                    let mut issues: Vec<::snugom::errors::ValidationIssue> = self.validation_issues.clone();
                    #(#builder_required_checks)*
                    if !issues.is_empty() {
                        return Err(::snugom::errors::ValidationError::new(issues));
                    }
                    #(#builder_value_bindings)*
                    let entity = #name {
                        #(#builder_field_names),*
                    };
                    entity.validate()?;
                    Ok(entity)
                }

                pub fn build_payload(
                    mut self,
                ) -> ::snugom::errors::ValidationResult<::snugom::repository::MutationPayload> {
                    let entity = self.clone().build()?;
                    let entity_id = entity.#id_ident.clone();
                    let descriptor = <#name as ::snugom::types::EntityMetadata>::entity_descriptor();
                    let mut relations = self.relations;
                    let mut nested = self.nested_creates;
                    if !nested.is_empty() {
                        ::snugom::repository::link_nested_to_parent(&descriptor, &entity_id, &mut nested);
                    }
                    let payload = ::serde_json::to_value(&entity).map_err(|err| {
                        ::snugom::errors::ValidationError::single(
                            "__payload",
                            "serialization.failed",
                            err.to_string(),
                        )
                    })?;
                    let mirrors = entity.datetime_mirrors();
                    let idempotency_key = self.idempotency_key.take();
                    let idempotency_ttl = self.idempotency_ttl.take();
                    let managed_overrides = self.managed_overrides.into_iter().collect();
                    Ok(::snugom::repository::MutationPayload {
                        entity_id,
                        payload,
                        mirrors,
                        relations,
                        nested,
                        idempotency_key,
                        idempotency_ttl,
                        managed_overrides,
                    })
                }

                pub fn validate(&self) -> ::snugom::errors::ValidationResult<()> {
                    self.clone().build().map(|_| ())
                }
            }

            #[derive(Debug, Clone, Default)]
            #vis struct #patch_builder_ident {
                entity_id: ::std::option::Option<::std::string::String>,
                expected_version: ::std::option::Option<u64>,
                idempotency_key: ::std::option::Option<::std::string::String>,
                idempotency_ttl: ::std::option::Option<u64>,
                operations: ::std::vec::Vec<::snugom::repository::PatchOperation>,
                relations: ::std::vec::Vec<::snugom::repository::RelationPlan>,
                nested_creates: ::std::vec::Vec<::snugom::repository::NestedMutation>,
                validation_issues: ::std::vec::Vec<::snugom::errors::ValidationIssue>,
            }

            impl #patch_builder_ident {
                pub fn new() -> Self {
                    Self::default()
                }

                pub fn entity_id(mut self, value: impl Into<::std::string::String>) -> Self {
                    self.entity_id = Some(value.into());
                    self
                }

                pub fn expected_version(mut self, value: u64) -> Self {
                    self.expected_version = Some(value);
                    self
                }

                pub fn clear_expected_version(mut self) -> Self {
                    self.expected_version = None;
                    self
                }

                pub fn idempotency_key(mut self, value: impl Into<::std::string::String>) -> Self {
                    self.idempotency_key = Some(value.into());
                    self
                }

                pub fn set_idempotency_key(&mut self, value: impl Into<::std::string::String>) -> &mut Self {
                    self.idempotency_key = Some(value.into());
                    self
                }

                pub fn idempotency_ttl(mut self, ttl_seconds: u64) -> Self {
                    self.idempotency_ttl = Some(ttl_seconds);
                    self
                }

                pub fn set_idempotency_ttl(&mut self, ttl_seconds: u64) -> &mut Self {
                    self.idempotency_ttl = Some(ttl_seconds);
                    self
                }

                pub fn clear_idempotency(mut self) -> Self {
                    self.idempotency_key = None;
                    self.idempotency_ttl = None;
                    self
                }

                pub fn clear_idempotency_mut(&mut self) -> &mut Self {
                    self.idempotency_key = None;
                    self.idempotency_ttl = None;
                    self
                }

                #(#patch_setters)*

                pub fn connect(mut self, alias: impl Into<String>, values: Vec<String>) -> Self {
                    self.relations
                        .push(::snugom::repository::RelationPlan::new(alias, values, Vec::new()));
                    self
                }

                pub fn connect_mut(&mut self, alias: impl Into<String>, values: Vec<String>) -> &mut Self {
                    self.relations
                        .push(::snugom::repository::RelationPlan::new(alias, values, Vec::new()));
                    self
                }

                pub fn disconnect(mut self, alias: impl Into<String>, values: Vec<String>) -> Self {
                    self.relations
                        .push(::snugom::repository::RelationPlan::new(alias, Vec::new(), values));
                    self
                }

                pub fn disconnect_mut(&mut self, alias: impl Into<String>, values: Vec<String>) -> &mut Self {
                    self.relations
                        .push(::snugom::repository::RelationPlan::new(alias, Vec::new(), values));
                    self
                }

                pub fn delete(mut self, alias: impl Into<String>, values: Vec<String>) -> Self {
                    let mut plan = ::snugom::repository::RelationPlan::new(alias, Vec::new(), Vec::new());
                    plan.delete = values;
                    self.relations.push(plan);
                    self
                }

                pub fn delete_mut(&mut self, alias: impl Into<String>, values: Vec<String>) -> &mut Self {
                    let mut plan = ::snugom::repository::RelationPlan::new(alias, Vec::new(), Vec::new());
                    plan.delete = values;
                    self.relations.push(plan);
                    self
                }

                pub fn create_relation<B>(mut self, alias: impl Into<String>, builder: B) -> Self
                where
                    B: ::snugom::repository::MutationPayloadBuilder,
                    <B as ::snugom::repository::MutationPayloadBuilder>::Entity: ::snugom::types::EntityMetadata,
                {
                    let alias_string = alias.into();
                    match builder.into_payload() {
                        Ok(payload) => {
                            <B::Entity as ::snugom::types::EntityMetadata>::ensure_registered();
                            let entity_id = payload.entity_id.clone();
                            self.relations.push(::snugom::repository::RelationPlan::new(
                                alias_string.clone(),
                                vec![entity_id],
                                Vec::new(),
                            ));
                            self.nested_creates.push(::snugom::repository::NestedMutation {
                                alias: alias_string,
                                descriptor: <B::Entity as ::snugom::types::EntityMetadata>::entity_descriptor(),
                                payload,
                            });
                        }
                        Err(err) => {
                            self.validation_issues.extend(err.issues.into_iter());
                        }
                    }
                    self
                }

                pub fn create_relation_mut<B>(
                    &mut self,
                    alias: impl Into<String>,
                    builder: B,
                ) -> &mut Self
                where
                    B: ::snugom::repository::MutationPayloadBuilder,
                    <B as ::snugom::repository::MutationPayloadBuilder>::Entity: ::snugom::types::EntityMetadata,
                {
                    let alias_string = alias.into();
                    match builder.into_payload() {
                        Ok(payload) => {
                            <B::Entity as ::snugom::types::EntityMetadata>::ensure_registered();
                            let entity_id = payload.entity_id.clone();
                            self.relations.push(::snugom::repository::RelationPlan::new(
                                alias_string.clone(),
                                vec![entity_id],
                                Vec::new(),
                            ));
                            self.nested_creates.push(::snugom::repository::NestedMutation {
                                alias: alias_string,
                                descriptor: <B::Entity as ::snugom::types::EntityMetadata>::entity_descriptor(),
                                payload,
                            });
                        }
                        Err(err) => {
                            self.validation_issues.extend(err.issues.into_iter());
                        }
                    }
                    self
                }

                pub fn build_patch(mut self) -> ::snugom::errors::ValidationResult<::snugom::repository::MutationPatch> {
                    if self.entity_id.is_none() {
                        self.validation_issues.push(::snugom::errors::ValidationIssue::new(
                            "entity_id",
                            "validation.required",
                            "entity_id is required for patch operations",
                        ));
                    }
                    if self.operations.is_empty() && self.relations.is_empty() {
                        self.validation_issues.push(::snugom::errors::ValidationIssue::new(
                            "operations",
                            "validation.required",
                            "no fields or relation directives were provided for update",
                        ));
                    }
                    if !self.validation_issues.is_empty() {
                        return Err(::snugom::errors::ValidationError::new(self.validation_issues));
                    }

                    let entity_id = self.entity_id.take().unwrap();
                    let mut relations = self.relations;
                    let mut nested = self.nested_creates;
                    if !nested.is_empty() {
                        let descriptor = <#name as ::snugom::types::EntityMetadata>::entity_descriptor();
                        ::snugom::repository::link_nested_to_parent(&descriptor, &entity_id, &mut nested);
                    }

                    let idempotency_key = self.idempotency_key.take();
                    let idempotency_ttl = self.idempotency_ttl.take();

                    Ok(::snugom::repository::MutationPatch {
                        entity_id,
                        expected_version: self.expected_version,
                        operations: self.operations,
                        relations,
                        nested,
                        idempotency_key,
                        idempotency_ttl,
                    })
                }
            }

            impl ::snugom::repository::MutationPayloadBuilder for #builder_ident
            where
                #name: ::serde::Serialize + ::snugom::types::EntityMetadata,
            {
                type Entity = #name;

                fn into_payload(
                    self,
                ) -> ::snugom::errors::ValidationResult<::snugom::repository::MutationPayload> {
                    self.build_payload()
                }
            }

            impl #name {
                pub fn validation_builder() -> #builder_ident {
                    #builder_ident::default()
                }

                pub fn validate_builder(builder: &#builder_ident) -> ::snugom::errors::ValidationResult<()> {
                    builder.clone().build().map(|_| ())
                }

                pub fn patch_builder() -> #patch_builder_ident {
                    #patch_builder_ident::default()
                }
            }

            impl ::snugom::repository::UpdatePatchBuilder for #patch_builder_ident
            where
                #name: ::snugom::types::EntityMetadata,
            {
                type Entity = #name;

                fn into_patch(
                    self,
                ) -> ::snugom::errors::ValidationResult<::snugom::repository::MutationPatch> {
                    self.build_patch()
                }
            }

            #search_entity_impl
        }
    }

    /// Generate the impl SearchEntity if there are any indexed fields
    fn emit_search_entity(&self) -> TokenStream2 {
        // Check if we have any indexed fields
        let has_indexed_fields = self.fields.iter().any(|f| f.has_index());
        if !has_indexed_fields {
            return quote! {};
        }

        let name = &self.name;

        // Generate index schema static
        let index_schema_ident = format_ident!("__SNUGOM_INDEX_SCHEMA_{}", self.name.to_string().to_uppercase());
        let index_fields: Vec<_> = self.fields
            .iter()
            .filter_map(|f| f.to_index_field_tokens())
            .collect();
        let index_field_count = index_fields.len();

        // Generate sort fields static
        let sort_fields_ident = format_ident!("__SNUGOM_SORT_FIELDS_{}", self.name.to_string().to_uppercase());
        let sort_fields: Vec<_> = self.fields
            .iter()
            .filter_map(|f| f.to_sort_field_tokens())
            .collect();
        let sort_field_count = sort_fields.len();

        // Generate text search fields
        let text_fields: Vec<_> = self.fields
            .iter()
            .filter(|f| f.is_text_searchable())
            .map(|f| f.index_field_name())
            .collect();
        let text_field_count = text_fields.len();

        // Generate filter match arms
        let filter_arms: Vec<_> = self.fields
            .iter()
            .filter_map(|f| f.to_filter_match_arm())
            .collect();

        // Default sort logic
        let default_sort_expr = if let Some(ref ds) = self.default_sort {
            // Find the matching sort field
            let field_name = &ds.field;
            let descending = ds.descending;
            quote! {
                static DEFAULT: ::std::sync::OnceLock<::snugom::search::SortField> = ::std::sync::OnceLock::new();
                DEFAULT.get_or_init(|| {
                    // Find the sort field by name
                    #sort_fields_ident.iter()
                        .find(|f| f.name == #field_name)
                        .cloned()
                        .map(|mut f| {
                            if #descending {
                                f.default_order = ::snugom::search::SortOrder::Desc;
                            }
                            f
                        })
                        .unwrap_or_else(|| #sort_fields_ident[0].clone())
                })
            }
        } else if sort_field_count > 0 {
            quote! { &#sort_fields_ident[0] }
        } else {
            // No sortable fields - this is a compile error case but we'll handle it gracefully
            quote! {
                static EMPTY: ::snugom::search::SortField = ::snugom::search::SortField {
                    name: "",
                    path: "",
                    default_order: ::snugom::search::SortOrder::Asc,
                };
                &EMPTY
            }
        };

        quote! {
            #[allow(non_upper_case_globals)]
            static #index_schema_ident: [::snugom::search::IndexField; #index_field_count] = [
                #(#index_fields),*
            ];

            #[allow(non_upper_case_globals)]
            static #sort_fields_ident: [::snugom::search::SortField; #sort_field_count] = [
                #(#sort_fields),*
            ];

            impl ::snugom::search::SearchEntity for #name {
                fn index_definition(prefix: &str) -> ::snugom::search::IndexDefinition {
                    let service = <#name as ::snugom::types::BundleRegistered>::SERVICE;
                    let collection = <#name as ::snugom::types::BundleRegistered>::COLLECTION;
                    ::snugom::search::IndexDefinition {
                        name: format!("{}:{}:{}:idx", prefix, service, collection),
                        prefixes: vec![format!("{}:{}:{}:", prefix, service, collection)],
                        filter: None,
                        schema: &#index_schema_ident,
                    }
                }

                fn allowed_sorts() -> &'static [::snugom::search::SortField] {
                    &#sort_fields_ident
                }

                fn default_sort() -> &'static ::snugom::search::SortField {
                    #default_sort_expr
                }

                fn text_search_fields() -> &'static [&'static str] {
                    static FIELDS: [&str; #text_field_count] = [#(#text_fields),*];
                    &FIELDS
                }

                fn map_filter(
                    descriptor: ::snugom::search::FilterDescriptor,
                ) -> Result<::snugom::search::FilterCondition, ::snugom::errors::RepoError> {
                    match descriptor.field.as_str() {
                        #(#filter_arms,)*
                        other => Err(::snugom::errors::RepoError::InvalidRequest {
                            message: format!("Unknown filter field: {}", other),
                        }),
                    }
                }
            }
        }
    }
}

impl ParsedRelation {
    fn to_tokens(&self) -> TokenStream2 {
        let alias = &self.alias;
        let target = &self.target;
        let kind = match self.kind {
            RelationKind::HasMany => quote! { ::snugom::types::RelationKind::HasMany },
            RelationKind::ManyToMany => quote! { ::snugom::types::RelationKind::ManyToMany },
            RelationKind::BelongsTo => quote! { ::snugom::types::RelationKind::BelongsTo },
        };
        let cascade = match self.cascade {
            CascadePolicy::Delete => quote! { ::snugom::types::CascadePolicy::Delete },
            CascadePolicy::Detach => quote! { ::snugom::types::CascadePolicy::Detach },
            CascadePolicy::None => quote! { ::snugom::types::CascadePolicy::None },
        };
        let foreign_key = match &self.foreign_key {
            Some(value) => quote! { ::std::option::Option::Some(#value.to_string()) },
            None => quote! { ::std::option::Option::None },
        };
        quote! {
            ::snugom::types::RelationDescriptor {
                alias: #alias.to_string(),
                target: #target.to_string(),
                target_service: None,
                kind: #kind,
                cascade: #cascade,
                foreign_key: #foreign_key,
            }
        }
    }
}

impl ParsedEntity {
    fn detect_derived_id(fields: &[ParsedField], relations: &[ParsedRelation]) -> Option<DerivedIdSpec> {
        let id_field = fields.iter().find(|field| field.is_id)?;
        if !matches!(id_field.ty.base, FieldBase::String) {
            return None;
        }
        let id_field_name = id_field.name.clone();

        let mut belongs_to_relations: Vec<&ParsedRelation> = relations
            .iter()
            .filter(|relation| matches!(relation.kind, RelationKind::BelongsTo) && relation.foreign_key.is_some())
            .collect();

        if belongs_to_relations.len() != 1 {
            return None;
        }

        let relation = belongs_to_relations.pop().expect("length checked");
        let foreign_key = relation.foreign_key.clone()?;
        let foreign_field = fields.iter().find(|field| field.name == foreign_key)?;
        if !matches!(foreign_field.ty.base, FieldBase::String) {
            return None;
        }

        let mut candidates = fields
            .iter()
            .filter(|field| {
                field.name.ends_with("_id")
                    && field.name != foreign_key
                    && field.name != "tenant_id"
                    && field.name != id_field_name
                    && matches!(field.ty.base, FieldBase::String)
            })
            .collect::<Vec<_>>();

        if candidates.is_empty() {
            return None;
        }

        let suffix_field = candidates.remove(0).name.clone();

        Some(DerivedIdSpec {
            components: vec![foreign_key, suffix_field],
            separator: ":".to_string(),
        })
    }
}
