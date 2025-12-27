impl FieldValidation {
fn emit_field_check(&self, field: &ParsedField, field_idents: &[Ident]) -> TokenStream2 {
        let field_ident = &field.ident;
        let field_name = &field.name;
        let optional = field.ty.optional;
        match &self.data {
            ValidationData::Length { min, max } => {
                let len_expr = match field.ty.base {
                    FieldBase::String => quote! { value.chars().count() },
                    FieldBase::Vec => quote! { value.len() },
                    _ => quote! { 0 },
                };
                let min_check = min.map(|value| {
                    quote! {
                        if len < #value {
                            issues.push(::snugom::errors::ValidationIssue::new(
                                #field_name,
                                "validation.length",
                                format!("length must be at least {}", #value),
                            ));
                        }
                    }
                });
                let max_check = max.map(|value| {
                    quote! {
                        if len > #value {
                            issues.push(::snugom::errors::ValidationIssue::new(
                                #field_name,
                                "validation.length",
                                format!("length must be at most {}", #value),
                            ));
                        }
                    }
                });
                if optional {
                    quote! {
                        if let Some(value) = self.#field_ident.as_ref() {
                            let len = #len_expr;
                            #min_check
                            #max_check
                        }
                    }
                } else {
                    quote! {
                        {
                            let value = &self.#field_ident;
                            let len = #len_expr;
                            #min_check
                            #max_check
                        }
                    }
                }
            }
            ValidationData::Range {
                min,
                min_repr,
                max,
                max_repr,
            } => {
                let min_check = if let Some(tokens) = min {
                    let repr = min_repr.as_deref().unwrap_or("min");
                    quote! {
                        if value < #tokens {
                            issues.push(::snugom::errors::ValidationIssue::new(
                                #field_name,
                                "validation.range",
                                format!("value must be at least {}", #repr),
                            ));
                        }
                    }
                } else {
                    quote! {}
                };
                let max_check = if let Some(tokens) = max {
                    let repr = max_repr.as_deref().unwrap_or("max");
                    quote! {
                        if value > #tokens {
                            issues.push(::snugom::errors::ValidationIssue::new(
                                #field_name,
                                "validation.range",
                                format!("value must be at most {}", #repr),
                            ));
                        }
                    }
                } else {
                    quote! {}
                };
                if optional {
                    quote! {
                        if let Some(value) = self.#field_ident {
                            let value = value;
                            #min_check
                            #max_check
                        }
                    }
                } else {
                    quote! {
                        {
                            let value = self.#field_ident;
                            #min_check
                            #max_check
                        }
                    }
                }
            }
            ValidationData::Regex { pattern } => {
                let lit = LitStr::new(pattern, Span::call_site());
                if optional {
                    quote! {
                        if let Some(value) = self.#field_ident.as_ref() {
                            if !::regex::Regex::new(#lit).unwrap().is_match(value.as_str()) {
                                issues.push(::snugom::errors::ValidationIssue::new(
                                    #field_name,
                                    "validation.regex",
                                    format!("value does not match pattern {}", #lit),
                                ));
                            }
                        }
                    }
                } else {
                    quote! {
                        {
                            let value = &self.#field_ident;
                            if !::regex::Regex::new(#lit).unwrap().is_match(value.as_str()) {
                                issues.push(::snugom::errors::ValidationIssue::new(
                                    #field_name,
                                    "validation.regex",
                                    format!("value does not match pattern {}", #lit),
                                ));
                            }
                        }
                    }
                }
            }
            ValidationData::Enum {
                allowed,
                case_insensitive,
            } => {
                let normalized: Vec<String> = allowed
                    .iter()
                    .map(|value| {
                        if *case_insensitive {
                            value.to_ascii_lowercase()
                        } else {
                            value.clone()
                        }
                    })
                    .collect();
                let allowed_tokens: Vec<TokenStream2> = normalized
                    .iter()
                    .map(|value| {
                        let lit = LitStr::new(value, Span::call_site());
                        quote! { #lit }
                    })
                    .collect();
                let allowed_for_array = allowed_tokens.clone();
                if optional {
                    quote! {
                        if let Some(value) = self.#field_ident.as_ref() {
                            let candidate = if #case_insensitive {
                                value.to_ascii_lowercase()
                            } else {
                                value.clone()
                            };
                            let allowed_values = [#(#allowed_for_array),*];
                            if !allowed_values.iter().any(|allowed| allowed == &candidate) {
                                issues.push(::snugom::errors::ValidationIssue::new(
                                    #field_name,
                                    "validation.enum",
                                    format!("value must be one of {:?}", &allowed_values),
                                ));
                            }
                        }
                    }
                } else {
                    quote! {
                        {
                            let candidate = if #case_insensitive {
                                self.#field_ident.to_ascii_lowercase()
                            } else {
                                self.#field_ident.clone()
                            };
                            let allowed_values = [#(#allowed_for_array),*];
                            if !allowed_values.iter().any(|allowed| allowed == &candidate) {
                                issues.push(::snugom::errors::ValidationIssue::new(
                                    #field_name,
                                    "validation.enum",
                                    format!("value must be one of {:?}", &allowed_values),
                                ));
                            }
                        }
                    }
                }
            }
            ValidationData::Email => {
                if optional {
                    quote! {
                        if let Some(value) = self.#field_ident.as_ref() {
                            if !::snugom::validators::is_valid_email(value.as_str()) {
                                issues.push(::snugom::errors::ValidationIssue::new(
                                    #field_name,
                                    "validation.email",
                                    "value must be a valid email address",
                                ));
                            }
                        }
                    }
                } else {
                    quote! {
                        {
                            if !::snugom::validators::is_valid_email(self.#field_ident.as_str()) {
                                issues.push(::snugom::errors::ValidationIssue::new(
                                    #field_name,
                                    "validation.email",
                                    "value must be a valid email address",
                                ));
                            }
                        }
                    }
                }
            }
            ValidationData::Url => {
                if optional {
                    quote! {
                        if let Some(value) = self.#field_ident.as_ref() {
                            if !::snugom::validators::is_valid_url(value.as_str()) {
                                issues.push(::snugom::errors::ValidationIssue::new(
                                    #field_name,
                                    "validation.url",
                                    "value must be a valid URL",
                                ));
                            }
                        }
                    }
                } else {
                    quote! {
                        {
                            if !::snugom::validators::is_valid_url(self.#field_ident.as_str()) {
                                issues.push(::snugom::errors::ValidationIssue::new(
                                    #field_name,
                                    "validation.url",
                                    "value must be a valid URL",
                                ));
                            }
                        }
                    }
                }
            }
            ValidationData::Uuid => {
                if optional {
                    quote! {
                        if let Some(value) = self.#field_ident.as_ref() {
                            if !::snugom::validators::is_valid_uuid(value.as_str()) {
                                issues.push(::snugom::errors::ValidationIssue::new(
                                    #field_name,
                                    "validation.uuid",
                                    "value must be a valid UUID",
                                ));
                            }
                        }
                    }
                } else {
                    quote! {
                        {
                            if !::snugom::validators::is_valid_uuid(self.#field_ident.as_str()) {
                                issues.push(::snugom::errors::ValidationIssue::new(
                                    #field_name,
                                    "validation.uuid",
                                    "value must be a valid UUID",
                                ));
                            }
                        }
                    }
                }
            }
            ValidationData::RequiredIf { expr, .. } => {
                if !field.ty.optional {
                    return quote! {};
                }
                let bindings = destructure_fields(field_idents);
                quote! {
                    let condition = {
                        #[allow(unused_variables)]
                        {
                            #bindings
                            #expr
                        }
                    };
                    if condition && self.#field_ident.is_none() {
                        issues.push(::snugom::errors::ValidationIssue::new(
                            #field_name,
                            "validation.required_if",
                            "field is required when condition is met",
                        ));
                    }
                }
            }
            ValidationData::ForbiddenIf { expr, .. } => {
                if !field.ty.optional {
                    return quote! {};
                }
                let bindings = destructure_fields(field_idents);
                quote! {
                    let condition = {
                        #[allow(unused_variables)]
                        {
                            #bindings
                            #expr
                        }
                    };
                    if condition && self.#field_ident.is_some() {
                        issues.push(::snugom::errors::ValidationIssue::new(
                            #field_name,
                            "validation.forbidden_if",
                            "field must be absent when condition is met",
                        ));
                    }
                }
            }
            ValidationData::Unique { .. } => {
                // Unique constraints are enforced at database level via Lua script,
                // not in local validation. This is because uniqueness requires
                // checking against all other entities in the collection.
                // This code path is only reached for Vec element uniqueness validation
                // which we don't support in the same way - skip it here.
                quote! {}
            }
            ValidationData::Custom { path, .. } => {
                let call = quote! { #path(&self.#field_ident) };
                if optional {
                    quote! {
                        if let Some(value) = self.#field_ident.as_ref() {
                            if let Err(err) = #path(value) {
                                let err = err;
                                issues.extend(err.issues.into_iter());
                            }
                        }
                    }
                } else {
                    quote! {
                        if let Err(err) = #call {
                            let err = err;
                            issues.extend(err.issues.into_iter());
                        }
                    }
                }
            }
        }
    }

    fn emit_each_check(&self, field: &ParsedField) -> TokenStream2 {
        let field_ident = &field.ident;
        let field_name = &field.name;
        let optional = field.ty.optional;
        let index_ident = format_ident!("idx");
        let item_ident = format_ident!("item");
        let path_expr = quote! { format!("{}[{}]", #field_name, #index_ident) };
        let inner = match &self.data {
            ValidationData::Length { min, max } => {
                let len_expr = quote! { #item_ident.chars().count() };
                let min_check = min.map(|value| {
                    quote! {
                        if len < #value {
                            issues.push(::snugom::errors::ValidationIssue::new(
                                #path_expr.clone(),
                                "validation.length",
                                format!("length must be at least {}", #value),
                            ));
                        }
                    }
                });
                let max_check = max.map(|value| {
                    quote! {
                        if len > #value {
                            issues.push(::snugom::errors::ValidationIssue::new(
                                #path_expr.clone(),
                                "validation.length",
                                format!("length must be at most {}", #value),
                            ));
                        }
                    }
                });
                quote! {
                    let len = #len_expr;
                    #min_check
                    #max_check
                }
            }
            ValidationData::Range {
                min,
                min_repr,
                max,
                max_repr,
            } => {
                let min_check = if let Some(tokens) = min {
                    let repr = min_repr.as_deref().unwrap_or("min");
                    quote! {
                        if #item_ident < #tokens {
                            issues.push(::snugom::errors::ValidationIssue::new(
                                #path_expr.clone(),
                                "validation.range",
                                format!("value must be at least {}", #repr),
                            ));
                        }
                    }
                } else {
                    quote! {}
                };
                let max_check = if let Some(tokens) = max {
                    let repr = max_repr.as_deref().unwrap_or("max");
                    quote! {
                        if #item_ident > #tokens {
                            issues.push(::snugom::errors::ValidationIssue::new(
                                #path_expr.clone(),
                                "validation.range",
                                format!("value must be at most {}", #repr),
                            ));
                        }
                    }
                } else {
                    quote! {}
                };
                quote! {
                    #min_check
                    #max_check
                }
            }
            ValidationData::Regex { pattern } => {
                let lit = LitStr::new(pattern, Span::call_site());
                quote! {
                    if !::regex::Regex::new(#lit).unwrap().is_match(#item_ident.as_str()) {
                        issues.push(::snugom::errors::ValidationIssue::new(
                            #path_expr.clone(),
                            "validation.regex",
                            format!("value does not match pattern {}", #lit),
                        ));
                    }
                }
            }
            ValidationData::Enum {
                allowed,
                case_insensitive,
            } => {
                let normalized: Vec<String> = allowed
                    .iter()
                    .map(|value| {
                        if *case_insensitive {
                            value.to_ascii_lowercase()
                        } else {
                            value.clone()
                        }
                    })
                    .collect();
                let allowed_tokens: Vec<TokenStream2> = normalized
                    .iter()
                    .map(|value| {
                        let lit = LitStr::new(value, Span::call_site());
                        quote! { #lit }
                    })
                    .collect();
                let allowed_for_array = allowed_tokens.clone();
                quote! {
                    let candidate = if #case_insensitive {
                        #item_ident.to_ascii_lowercase()
                    } else {
                        #item_ident.clone()
                    };
                    let allowed_values = [#(#allowed_for_array),*];
                    if !allowed_values.iter().any(|allowed| allowed == &candidate) {
                        issues.push(::snugom::errors::ValidationIssue::new(
                            #path_expr.clone(),
                            "validation.enum",
                            format!("value must be one of {:?}", &allowed_values),
                        ));
                    }
                }
            }
            ValidationData::Email => {
                quote! {
                    if !::snugom::validators::is_valid_email(#item_ident.as_str()) {
                        issues.push(::snugom::errors::ValidationIssue::new(
                            #path_expr.clone(),
                            "validation.email",
                            "value must be a valid email address",
                        ));
                    }
                }
            }
            ValidationData::Url => {
                quote! {
                    if !::snugom::validators::is_valid_url(#item_ident.as_str()) {
                        issues.push(::snugom::errors::ValidationIssue::new(
                            #path_expr.clone(),
                            "validation.url",
                            "value must be a valid URL",
                        ));
                    }
                }
            }
            ValidationData::Uuid => {
                quote! {
                    if !::snugom::validators::is_valid_uuid(#item_ident.as_str()) {
                        issues.push(::snugom::errors::ValidationIssue::new(
                            #path_expr.clone(),
                            "validation.uuid",
                            "value must be a valid UUID",
                        ));
                    }
                }
            }
            ValidationData::Custom { path, .. } => {
                quote! {
                    if let Err(err) = #path(#item_ident) {
                        let err = err;
                        for issue in err.issues {
                            issues.push(::snugom::errors::ValidationIssue::new(
                                #path_expr.clone(),
                                issue.code,
                                issue.message,
                            ));
                        }
                    }
                }
            }
            ValidationData::RequiredIf { .. } | ValidationData::ForbiddenIf { .. } | ValidationData::Unique { .. } => {
                return quote! {};
            }
        };

        if optional {
            quote! {
                if let Some(values) = self.#field_ident.as_ref() {
                    for (#index_ident, #item_ident) in values.iter().enumerate() {
                        #inner
                    }
                }
            }
        } else {
            quote! {
                for (#index_ident, #item_ident) in self.#field_ident.iter().enumerate() {
                    #inner
                }
            }
        }
    }
}

fn parse_validation_rule(
    rule: ParseNestedMeta,
    ty: &TypeInfo,
    validations: &mut Vec<FieldValidation>,
    field_name: &str,
) -> Result<()> {
    let ident = rule
        .path
        .get_ident()
        .cloned()
        .ok_or_else(|| Error::new(rule.path.span(), format!("unsupported validator on `{}`", field_name)))?;
    let ident_str = ident.to_string();
    match ident_str.as_str() {
        "length" => {
            ensure_length_supported(ty.base, rule.path.span())?;
            let mut min = None;
            let mut max = None;
            rule.parse_nested_meta(|item| {
                if item.path.is_ident("min") {
                    let lit: LitInt = item.value()?.parse()?;
                    min = Some(lit.base10_parse()?);
                } else if item.path.is_ident("max") {
                    let lit: LitInt = item.value()?.parse()?;
                    max = Some(lit.base10_parse()?);
                }
                Ok(())
            })?;
            validations.push(FieldValidation {
                scope: ValidationScope::Field,
                data: ValidationData::Length { min, max },
            });
        }
        "range" => {
            ensure_range_supported(ty.base, rule.path.span())?;
            let mut min = None;
            let mut min_repr = None;
            let mut max = None;
            let mut max_repr = None;
            rule.parse_nested_meta(|item| {
                if item.path.is_ident("min") {
                    let expr: Expr = item.value()?.parse()?;
                    let tokens = expr.to_token_stream();
                    min_repr = Some(tokens.to_string());
                    min = Some(tokens);
                } else if item.path.is_ident("max") {
                    let expr: Expr = item.value()?.parse()?;
                    let tokens = expr.to_token_stream();
                    max_repr = Some(tokens.to_string());
                    max = Some(tokens);
                }
                Ok(())
            })?;
            validations.push(FieldValidation {
                scope: ValidationScope::Field,
                data: ValidationData::Range {
                    min,
                    min_repr,
                    max,
                    max_repr,
                },
            });
        }
        "regex" => {
            ensure_string_supported(ty.base, rule.path.span(), "regex")?;
            let pattern: LitStr = rule.value()?.parse()?;
            ensure_valid_regex(&pattern.value(), pattern.span())?;
            validations.push(FieldValidation {
                scope: ValidationScope::Field,
                data: ValidationData::Regex {
                    pattern: pattern.value(),
                },
            });
        }
        "enum" => {
            ensure_string_supported(ty.base, rule.path.span(), "enum")?;
            let mut allowed = Vec::new();
            let mut case_insensitive = false;
            rule.parse_nested_meta(|item| {
                if item.path.is_ident("allowed") {
                    let array: ExprArray = item.value()?.parse()?;
                    for expr in array.elems {
                        match expr {
                            Expr::Lit(expr_lit) => match expr_lit.lit {
                                syn::Lit::Str(lit) => allowed.push(lit.value()),
                                _ => return Err(item.error("allowed expects string literals")),
                            },
                            _ => return Err(item.error("allowed expects string literals")),
                        }
                    }
                } else if item.path.is_ident("case_insensitive") {
                    if item.input.peek(syn::Token![=]) {
                        let lit: LitBool = item.value()?.parse()?;
                        case_insensitive = lit.value;
                    } else {
                        case_insensitive = true;
                    }
                }
                Ok(())
            })?;
            validations.push(FieldValidation {
                scope: ValidationScope::Field,
                data: ValidationData::Enum {
                    allowed,
                    case_insensitive,
                },
            });
        }
        "email" => {
            ensure_string_supported(ty.base, rule.path.span(), "email")?;
            validations.push(FieldValidation {
                scope: ValidationScope::Field,
                data: ValidationData::Email,
            });
        }
        "url" => {
            ensure_string_supported(ty.base, rule.path.span(), "url")?;
            validations.push(FieldValidation {
                scope: ValidationScope::Field,
                data: ValidationData::Url,
            });
        }
        "uuid" => {
            ensure_string_supported(ty.base, rule.path.span(), "uuid")?;
            validations.push(FieldValidation {
                scope: ValidationScope::Field,
                data: ValidationData::Uuid,
            });
        }
        "required_if" => {
            if !ty.optional {
                return Err(Error::new(
                    rule.path.span(),
                    "required_if validator requires an Option<T> field",
                ));
            }
            let expr_lit: LitStr = rule.value()?.parse()?;
            let expr_tokens = parse_condition_expr(&expr_lit)?;
            validations.push(FieldValidation {
                scope: ValidationScope::Field,
                data: ValidationData::RequiredIf {
                    expr: expr_tokens,
                    expr_repr: expr_lit.value(),
                },
            });
        }
        "forbidden_if" => {
            if !ty.optional {
                return Err(Error::new(
                    rule.path.span(),
                    "forbidden_if validator requires an Option<T> field",
                ));
            }
            let expr_lit: LitStr = rule.value()?.parse()?;
            let expr_tokens = parse_condition_expr(&expr_lit)?;
            validations.push(FieldValidation {
                scope: ValidationScope::Field,
                data: ValidationData::ForbiddenIf {
                    expr: expr_tokens,
                    expr_repr: expr_lit.value(),
                },
            });
        }
        "unique" => {
            // Parse optional case_insensitive flag: unique or unique(case_insensitive)
            let mut case_insensitive = false;
            if rule.input.peek(syn::token::Paren) {
                let content;
                syn::parenthesized!(content in rule.input);
                let inner: syn::Ident = content.parse()?;
                if inner == "case_insensitive" {
                    case_insensitive = true;
                } else {
                    return Err(Error::new(
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
        "each" => {
            ensure_vec_supported(ty.base, rule.path.span(), "each")?;
            let element = ensure_element_present(ty.element.clone(), rule.path.span(), "each")?;
            let spec: LitStr = rule.value()?.parse()?;
            let data = parse_inline_validator(spec.value(), element, rule.path.span())?;
            validations.push(FieldValidation {
                scope: ValidationScope::EachElement,
                data,
            });
        }
        "custom" => {
            let lit: LitStr = rule.value()?.parse()?;
            let path = syn::parse_str::<syn::Path>(&lit.value())
                .map_err(|err| Error::new(lit.span(), format!("invalid custom validator path: {}", err)))?;
            validations.push(FieldValidation {
                scope: ValidationScope::Field,
                data: ValidationData::Custom {
                    path: path.to_token_stream(),
                    path_repr: lit.value(),
                },
            });
        }
        _ => {
            return Err(Error::new(
                rule.path.span(),
                format!("unknown snugom validator `{}` on field `{}`", ident_str, field_name),
            ));
        }
    }
    Ok(())
}

fn classify_type(ty: &Type) -> TypeInfo {
    let ty_clone = ty.clone();

    if let Some(inner) = unwrap_option(ty) {
        let mut info = classify_type(inner);
        info.optional = true;
        info.ty = ty_clone;
        info.option_inner = Some(inner.clone());
        return info;
    }

    if let Some(inner) = unwrap_vec(ty) {
        let element_info = classify_type(inner);
        let type_name = extract_type_name(inner);
        return TypeInfo {
            optional: false,
            base: FieldBase::Vec,
            element: Some(ElementType {
                optional: element_info.optional,
                base: element_info.base,
                is_datetime: element_info.is_datetime,
                ty: inner.clone(),
                type_name,
            }),
            is_datetime: false,
            ty: ty_clone,
            option_inner: None,
        };
    }

    if is_string_type(ty) {
        return TypeInfo {
            optional: false,
            base: FieldBase::String,
            element: None,
            is_datetime: false,
            ty: ty_clone,
            option_inner: None,
        };
    }

    if is_numeric_type(ty) {
        return TypeInfo {
            optional: false,
            base: FieldBase::Numeric,
            element: None,
            is_datetime: false,
            ty: ty_clone,
            option_inner: None,
        };
    }

    if is_bool_type(ty) {
        return TypeInfo {
            optional: false,
            base: FieldBase::Boolean,
            element: None,
            is_datetime: false,
            ty: ty_clone,
            option_inner: None,
        };
    }

    if is_datetime_type(ty) {
        return TypeInfo {
            optional: false,
            base: FieldBase::Other,
            element: None,
            is_datetime: true,
            ty: ty_clone,
            option_inner: None,
        };
    }

    TypeInfo {
        optional: false,
        base: FieldBase::Other,
        element: None,
        is_datetime: false,
        ty: ty_clone,
        option_inner: None,
    }
}

fn unwrap_option(ty: &Type) -> Option<&Type> {
    match ty {
        Type::Path(path) if last_ident_str(path).as_deref() == Some("Option") => {
            match &path.path.segments.last().unwrap().arguments {
                syn::PathArguments::AngleBracketed(args) => args.args.first().and_then(|arg| match arg {
                    syn::GenericArgument::Type(inner) => Some(inner),
                    _ => None,
                }),
                _ => None,
            }
        }
        _ => None,
    }
}

fn unwrap_vec(ty: &Type) -> Option<&Type> {
    match ty {
        Type::Path(path) if last_ident_str(path).as_deref() == Some("Vec") => {
            match &path.path.segments.last().unwrap().arguments {
                syn::PathArguments::AngleBracketed(args) => args.args.first().and_then(|arg| match arg {
                    syn::GenericArgument::Type(inner) => Some(inner),
                    _ => None,
                }),
                _ => None,
            }
        }
        _ => None,
    }
}

fn last_ident_str(path: &syn::TypePath) -> Option<String> {
    path.path.segments.last().map(|seg| seg.ident.to_string())
}

/// Extract the type name from a Type (e.g., "GuildMember" from GuildMember)
fn extract_type_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(path) => last_ident_str(path),
        _ => None,
    }
}

/// Converts a PascalCase type name to snake_case plural form
/// e.g., "GuildMember" -> "guild_members"
fn to_snake_plural(name: &str) -> String {
    let snake = to_snake_case(name);
    pluralize(&snake)
}

/// Converts a PascalCase identifier to snake_case
fn to_snake_case(name: &str) -> String {
    let mut result = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }
    result
}

/// Simple pluralization rules
fn pluralize(word: &str) -> String {
    if word.ends_with('s') || word.ends_with('x') || word.ends_with("ch") || word.ends_with("sh") {
        format!("{word}es")
    } else if word.ends_with('y') && !word.ends_with("ay") && !word.ends_with("ey") && !word.ends_with("oy") && !word.ends_with("uy") {
        format!("{}ies", &word[..word.len() - 1])
    } else {
        format!("{word}s")
    }
}

fn is_string_type(ty: &Type) -> bool {
    match ty {
        Type::Path(path) => last_ident_str(path).map(|id| id == "String").unwrap_or(false),
        _ => false,
    }
}

fn is_numeric_type(ty: &Type) -> bool {
    match ty {
        Type::Path(path) => last_ident_str(path)
            .map(|id| {
                matches!(
                    id.as_str(),
                    "i8" | "i16"
                        | "i32"
                        | "i64"
                        | "i128"
                        | "u8"
                        | "u16"
                        | "u32"
                        | "u64"
                        | "u128"
                        | "f32"
                        | "f64"
                        | "isize"
                        | "usize"
                )
            })
            .unwrap_or(false),
        _ => false,
    }
}

fn is_bool_type(ty: &Type) -> bool {
    match ty {
        Type::Path(path) => last_ident_str(path).map(|id| id == "bool").unwrap_or(false),
        _ => false,
    }
}

fn is_datetime_type(ty: &Type) -> bool {
    match ty {
        Type::Path(path) => last_ident_str(path).map(|id| id == "DateTime").unwrap_or(false),
        _ => false,
    }
}

fn ensure_length_supported(base: FieldBase, span: Span) -> Result<()> {
    match base {
        FieldBase::String | FieldBase::Vec => Ok(()),
        _ => Err(Error::new(span, "length validator only supported for strings and collections")),
    }
}

fn ensure_range_supported(base: FieldBase, span: Span) -> Result<()> {
    match base {
        FieldBase::Numeric => Ok(()),
        _ => Err(Error::new(span, "range validator only supported for numeric fields")),
    }
}

fn ensure_string_supported(base: FieldBase, span: Span, validator: &str) -> Result<()> {
    match base {
        FieldBase::String => Ok(()),
        _ => Err(Error::new(
            span,
            format!("{} validator only supported for string fields", validator),
        )),
    }
}

fn ensure_vec_supported(base: FieldBase, span: Span, validator: &str) -> Result<()> {
    match base {
        FieldBase::Vec => Ok(()),
        _ => Err(Error::new(
            span,
            format!("{} validator only supported for collection fields", validator),
        )),
    }
}

fn ensure_element_present(element: Option<ElementType>, span: Span, validator: &str) -> Result<ElementType> {
    element.ok_or_else(|| Error::new(span, format!("{} validator requires a Vec<T> field", validator)))
}

fn ensure_valid_regex(pattern: &str, span: Span) -> Result<()> {
    regex::Regex::new(pattern)
        .map(|_| ())
        .map_err(|err| Error::new(span, format!("invalid regex pattern: {}", err)))
}
