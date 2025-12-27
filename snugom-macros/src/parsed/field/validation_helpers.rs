fn optional_usize_tokens(value: Option<usize>) -> TokenStream2 {
    match value {
        Some(v) => quote! { Some(#v) },
        None => quote! { None },
    }
}

fn optional_string_tokens(value: &Option<String>) -> TokenStream2 {
    match value {
        Some(v) => {
            let lit = LitStr::new(v, Span::call_site());
            quote! { Some(#lit.to_string()) }
        }
        None => quote! { None },
    }
}

type ValidatorArg = (Option<String>, String);
type ValidatorSpec = (String, Vec<ValidatorArg>);

fn parse_condition_expr(lit: &LitStr) -> Result<TokenStream2> {
    let expr: Expr = syn::parse_str(&lit.value())
        .map_err(|err| Error::new(lit.span(), format!("failed to parse expression: {}", err)))?;
    Ok(expr.to_token_stream())
}

fn parse_inline_validator(spec: String, element: ElementType, span: Span) -> Result<ValidationData> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err(Error::new(span, "each validator requires a specification"));
    }
    let (name, args) = split_validator_spec(spec)?;
    match name.as_str() {
        "length" => {
            ensure_string_supported(element.base, span, "length")?;
            let mut min = None;
            let mut max = None;
            for (key, value) in args {
                match key.as_deref() {
                    Some("min") => {
                        min = Some(parse_usize(&value, span)?);
                    }
                    Some("max") => {
                        max = Some(parse_usize(&value, span)?);
                    }
                    other => {
                        return Err(Error::new(
                            span,
                            format!("unknown argument `{}` for length validator", other.unwrap_or("<value>")),
                        ));
                    }
                }
            }
            Ok(ValidationData::Length { min, max })
        }
        "range" => {
            ensure_range_supported(element.base, span)?;
            let mut min = None;
            let mut min_repr = None;
            let mut max = None;
            let mut max_repr = None;
            for (key, value) in args {
                match key.as_deref() {
                    Some("min") => {
                        let expr: Expr = syn::parse_str(&value)
                            .map_err(|err| Error::new(span, format!("invalid min expression: {}", err)))?;
                        let tokens = expr.to_token_stream();
                        min_repr = Some(tokens.to_string());
                        min = Some(tokens);
                    }
                    Some("max") => {
                        let expr: Expr = syn::parse_str(&value)
                            .map_err(|err| Error::new(span, format!("invalid max expression: {}", err)))?;
                        let tokens = expr.to_token_stream();
                        max_repr = Some(tokens.to_string());
                        max = Some(tokens);
                    }
                    other => {
                        return Err(Error::new(
                            span,
                            format!("unknown argument `{}` for range validator", other.unwrap_or("<value>")),
                        ));
                    }
                }
            }
            Ok(ValidationData::Range {
                min,
                min_repr,
                max,
                max_repr,
            })
        }
        "regex" => {
            ensure_string_supported(element.base, span, "regex")?;
            if args.len() != 1 {
                return Err(Error::new(span, "regex validator expects a pattern"));
            }
            let pattern = args[0].1.trim_matches('"').replace("\\\"", "\"");
            ensure_valid_regex(&pattern, span)?;
            Ok(ValidationData::Regex { pattern })
        }
        "enum" => {
            ensure_string_supported(element.base, span, "enum")?;
            let mut allowed = Vec::new();
            let mut case_insensitive = false;
            for (key, value) in args {
                match key.as_deref() {
                    Some("allowed") => {
                        allowed = parse_string_list(&value, span)?;
                    }
                    Some("case_insensitive") => {
                        case_insensitive = parse_bool(&value, span)?;
                    }
                    other => {
                        return Err(Error::new(
                            span,
                            format!("unknown argument `{}` for enum validator", other.unwrap_or("<value>")),
                        ));
                    }
                }
            }
            Ok(ValidationData::Enum {
                allowed,
                case_insensitive,
            })
        }
        "email" => {
            ensure_string_supported(element.base, span, "email")?;
            Ok(ValidationData::Email)
        }
        "url" => {
            ensure_string_supported(element.base, span, "url")?;
            Ok(ValidationData::Url)
        }
        "uuid" => {
            ensure_string_supported(element.base, span, "uuid")?;
            Ok(ValidationData::Uuid)
        }
        "custom" => {
            let path = args
                .first()
                .ok_or_else(|| Error::new(span, "custom validator requires a function path"))?
                .1
                .clone();
            let parsed = syn::parse_str::<syn::Path>(&path)
                .map_err(|err| Error::new(span, format!("invalid custom validator path: {}", err)))?;
            Ok(ValidationData::Custom {
                path: parsed.to_token_stream(),
                path_repr: path,
            })
        }
        other => Err(Error::new(span, format!("unsupported each validator `{}`", other))),
    }
}

fn split_validator_spec(spec: &str) -> Result<ValidatorSpec> {
    let mut name = String::new();
    let mut args = Vec::new();
    let mut chars = spec.chars().peekable();
    while let Some(ch) = chars.peek() {
        if *ch == '(' {
            chars.next();
            break;
        }
        name.push(*ch);
        chars.next();
    }
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(Error::new(Span::call_site(), "validator name cannot be empty"));
    }
    let mut current = String::new();
    let mut depth = 0;
    for ch in chars {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                if depth == 0 {
                    if !current.trim().is_empty() {
                        args.push(parse_key_value(current.trim())?);
                    }
                    current.clear();
                    break;
                }
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                args.push(parse_key_value(current.trim())?);
                current.clear();
            }
            ch => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        args.push(parse_key_value(current.trim())?);
    }
    Ok((name, args))
}

fn parse_key_value(input: &str) -> Result<(Option<String>, String)> {
    if input.is_empty() {
        return Ok((None, String::new()));
    }
    if let Some(idx) = input.find('=') {
        let key = input[..idx].trim().to_string();
        let value = input[idx + 1..].trim().to_string();
        Ok((Some(key), value))
    } else {
        Ok((None, input.trim().to_string()))
    }
}

fn parse_usize(value: &str, span: Span) -> Result<usize> {
    value
        .parse::<usize>()
        .map_err(|err| Error::new(span, format!("invalid integer `{}`: {}", value, err)))
}

fn parse_bool(value: &str, span: Span) -> Result<bool> {
    value
        .parse::<bool>()
        .map_err(|err| Error::new(span, format!("invalid boolean `{}`: {}", value, err)))
}

fn parse_string_list(value: &str, span: Span) -> Result<Vec<String>> {
    let trimmed = value.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return Err(Error::new(span, "expected list syntax: [\"a\", \"b\"]"));
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    let mut results = Vec::new();
    for part in inner.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !trimmed.starts_with('"') || !trimmed.ends_with('"') {
            return Err(Error::new(span, "list elements must be string literals"));
        }
        let value = trimmed[1..trimmed.len() - 1].replace("\\\"", "\"");
        results.push(value);
    }
    Ok(results)
}

fn destructure_fields(field_idents: &[Ident]) -> TokenStream2 {
    if field_idents.is_empty() {
        quote! {}
    } else {
        let bindings = field_idents.iter().map(|ident| quote! { let #ident = &self.#ident; });
        quote! {
            #( #bindings )*
        }
    }
}
