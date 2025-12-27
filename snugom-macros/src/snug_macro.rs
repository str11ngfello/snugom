use super::*;

pub(crate) struct SnugInvocation {
    path: Path,
    options: Option<UpdateOptions>,
    entries: Vec<RelEntry>,
}

impl SnugInvocation {
    pub(crate) fn emit(&self) -> Result<TokenStream2> {
        let path = &self.path;
        if let Some(options) = &self.options {
            let mut steps: Vec<TokenStream2> = Vec::new();
            let entity_id = &options.entity_id;
            steps.push(quote! {
                builder = builder.entity_id(#entity_id);
            });

            if let Some(expected) = &options.expected_version {
                steps.push(quote! {
                    builder = builder.expected_version(#expected);
                });
            }

            if let Some(key) = &options.idempotency_key {
                steps.push(quote! {
                    builder = builder.idempotency_key(#key);
                });
            }

            if let Some(ttl) = &options.idempotency_ttl {
                steps.push(quote! {
                    builder = builder.idempotency_ttl(#ttl);
                });
            }

            for entry in &self.entries {
                match entry {
                    RelEntry::Field(field) => {
                        let name = &field.name;
                        let value = &field.value;
                        if field.optional {
                            steps.push(quote! {
                                if let ::std::option::Option::Some(__snug_value) = (#value) {
                                    builder = builder.#name(__snug_value);
                                }
                            });
                        } else {
                            steps.push(quote! {
                                builder = builder.#name(#value);
                            });
                        }
                    }
                    RelEntry::Relation(relation) => {
                        steps.extend(relation.emit()?);
                    }
                }
            }

            Ok(quote! {{
                let mut builder = #path::patch_builder();
                #(#steps)*
                builder
            }})
        } else {
            let mut steps: Vec<TokenStream2> = Vec::new();
            for entry in &self.entries {
                match entry {
                    RelEntry::Field(field) => {
                        let name = &field.name;
                        let value = &field.value;
                        if field.optional {
                            steps.push(quote! {
                                if let ::std::option::Option::Some(__snug_value) = (#value) {
                                    builder = builder.#name(__snug_value);
                                }
                            });
                        } else {
                            steps.push(quote! {
                                builder = builder.#name(#value);
                            });
                        }
                    }
                    RelEntry::Relation(relation) => {
                        steps.extend(relation.emit()?);
                    }
                }
            }

            Ok(quote! {{
                let mut builder = #path::validation_builder();
                #(#steps)*
                builder
            }})
        }
    }

    fn parse_with_path(path: Path, input: ParseStream) -> Result<Self> {
        let content;
        braced!(content in input);
        let entries = Self::parse_entries(&content)?;
        Ok(Self {
            path,
            options: None,
            entries,
        })
    }

    fn parse_entries(content: &ParseBuffer<'_>) -> Result<Vec<RelEntry>> {
        let mut entries = Vec::new();
        while !content.is_empty() {
            entries.push(content.parse()?);
            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            } else {
                break;
            }
        }
        Ok(entries)
    }
}

impl Parse for SnugInvocation {
    fn parse(input: ParseStream) -> Result<Self> {
        let path: Path = input.parse()?;
        let options = if input.peek(syn::token::Paren) {
            Some(UpdateOptions::parse(input)?)
        } else {
            None
        };
        let content;
        braced!(content in input);
        let entries = Self::parse_entries(&content)?;
        Ok(Self { path, options, entries })
    }
}

struct UpdateOptions {
    entity_id: Expr,
    expected_version: Option<Expr>,
    idempotency_key: Option<Expr>,
    idempotency_ttl: Option<Expr>,
}

impl UpdateOptions {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        parenthesized!(content in input);
        let mut entity_id_expr: Option<Expr> = None;
        let mut expected_version_expr: Option<Expr> = None;
        let mut idempotency_expr: Option<Expr> = None;
        let mut idempotency_ttl_expr: Option<Expr> = None;

        while !content.is_empty() {
            let ident: Ident = content.parse()?;
            content.parse::<Token![=]>()?;
            let expr: Expr = content.parse()?;
            match ident.to_string().as_str() {
                "entity_id" => entity_id_expr = Some(expr),
                "expected_version" => expected_version_expr = Some(expr),
                "idempotency_key" => idempotency_expr = Some(expr),
                "idempotency_ttl" => idempotency_ttl_expr = Some(expr),
                other => return Err(Error::new(ident.span(), format!("unknown option `{}`", other))),
            }
            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            } else {
                break;
            }
        }

        let entity_id = entity_id_expr.ok_or_else(|| Error::new(input.span(), "entity_id option is required"))?;

        Ok(Self {
            entity_id,
            expected_version: expected_version_expr,
            idempotency_key: idempotency_expr,
            idempotency_ttl: idempotency_ttl_expr,
        })
    }
}

enum RelEntry {
    Field(FieldEntry),
    Relation(RelationEntry),
}

impl Parse for RelEntry {
    fn parse(input: ParseStream) -> Result<Self> {
        let name: Ident = input.parse()?;
        let optional = if input.peek(Token![?]) {
            input.parse::<Token![?]>()?;
            true
        } else {
            false
        };
        input.parse::<Token![:]>()?;
        if input.peek(syn::token::Bracket) {
            if optional {
                return Err(Error::new(
                    name.span(),
                    "relation directives do not support optional field markers",
                ));
            }
            let mut connects = Vec::new();
            let mut disconnects = Vec::new();
            let mut deletes = Vec::new();
            let mut creates = Vec::new();
            let inner;
            bracketed!(inner in input);
            while !inner.is_empty() {
                let op: Ident = inner.parse()?;
                match op.to_string().as_str() {
                    "connect" => {
                        let expr: Expr = inner.parse()?;
                        connects.push(expr);
                    }
                    "disconnect" => {
                        let expr: Expr = inner.parse()?;
                        disconnects.push(expr);
                    }
                    "delete" => {
                        let expr: Expr = inner.parse()?;
                        deletes.push(expr);
                    }
                    "create" => {
                        let path: Path = inner.parse()?;
                        let invocation = SnugInvocation::parse_with_path(path, &inner)?;
                        creates.push(invocation);
                    }
                    other => return Err(Error::new(op.span(), format!("unknown relation op `{}`", other))),
                }
                if inner.peek(Token![,]) {
                    inner.parse::<Token![,]>()?;
                }
            }
            Ok(RelEntry::Relation(RelationEntry {
                alias: name,
                connects,
                disconnects,
                deletes,
                creates,
            }))
        } else {
            let value: Expr = input.parse()?;
            Ok(RelEntry::Field(FieldEntry { name, value, optional }))
        }
    }
}

struct FieldEntry {
    name: Ident,
    value: Expr,
    optional: bool,
}

struct RelationEntry {
    alias: Ident,
    connects: Vec<Expr>,
    disconnects: Vec<Expr>,
    deletes: Vec<Expr>,
    creates: Vec<SnugInvocation>,
}

impl RelationEntry {
    fn alias_literal(&self) -> LitStr {
        LitStr::new(&self.alias.to_string(), self.alias.span())
    }

    fn emit(&self) -> Result<Vec<TokenStream2>> {
        let alias_lit = self.alias_literal();
        let mut tokens = Vec::new();

        if !self.connects.is_empty() {
            let connects = self
                .connects
                .iter()
                .map(|expr| quote! { ::std::convert::Into::<String>::into(#expr) });
            tokens.push(quote! {
                builder = builder.connect(#alias_lit, ::std::vec![#(#connects),*]);
            });
        }

        if !self.disconnects.is_empty() {
            let disconnects = self
                .disconnects
                .iter()
                .map(|expr| quote! { ::std::convert::Into::<String>::into(#expr) });
            tokens.push(quote! {
                builder = builder.disconnect(#alias_lit, ::std::vec![#(#disconnects),*]);
            });
        }

        if !self.deletes.is_empty() {
            let deletes = self
                .deletes
                .iter()
                .map(|expr| quote! { ::std::convert::Into::<String>::into(#expr) });
            tokens.push(quote! {
                builder = builder.delete(#alias_lit, ::std::vec![#(#deletes),*]);
            });
        }

        for create in &self.creates {
            let builder_tokens = create.emit()?;
            tokens.push(quote! {
                builder = builder.create_relation(#alias_lit, #builder_tokens);
            });
        }

        Ok(tokens)
    }
}
