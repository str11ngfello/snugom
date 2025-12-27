impl FieldValidation {
    fn to_descriptor_tokens(&self) -> TokenStream2 {
        let scope = match self.scope {
            ValidationScope::Field => quote! { ::snugom::types::ValidationScope::Field },
            ValidationScope::EachElement => quote! { ::snugom::types::ValidationScope::EachElement },
        };
        let rule = match &self.data {
            ValidationData::Length { min, max } => {
                let min_tokens = optional_usize_tokens(*min);
                let max_tokens = optional_usize_tokens(*max);
                quote! { ::snugom::types::ValidationRule::Length { min: #min_tokens, max: #max_tokens } }
            }
            ValidationData::Range { min_repr, max_repr, .. } => {
                let min_tokens = optional_string_tokens(min_repr);
                let max_tokens = optional_string_tokens(max_repr);
                quote! { ::snugom::types::ValidationRule::Range { min: #min_tokens, max: #max_tokens } }
            }
            ValidationData::Regex { pattern } => {
                let lit = LitStr::new(pattern, Span::call_site());
                quote! { ::snugom::types::ValidationRule::Regex { pattern: #lit.to_string() } }
            }
            ValidationData::Enum {
                allowed,
                case_insensitive,
            } => {
                let allowed_tokens = allowed.iter().map(|value| {
                    let lit = LitStr::new(value, Span::call_site());
                    quote! { #lit.to_string() }
                });
                quote! {
                    ::snugom::types::ValidationRule::Enum {
                        allowed: vec![#(#allowed_tokens),*],
                        case_insensitive: #case_insensitive,
                    }
                }
            }
            ValidationData::Email => quote! { ::snugom::types::ValidationRule::Email },
            ValidationData::Url => quote! { ::snugom::types::ValidationRule::Url },
            ValidationData::Uuid => quote! { ::snugom::types::ValidationRule::Uuid },
            ValidationData::RequiredIf { expr_repr, .. } => {
                let lit = LitStr::new(expr_repr, Span::call_site());
                quote! { ::snugom::types::ValidationRule::RequiredIf { expr: #lit.to_string() } }
            }
            ValidationData::ForbiddenIf { expr_repr, .. } => {
                let lit = LitStr::new(expr_repr, Span::call_site());
                quote! { ::snugom::types::ValidationRule::ForbiddenIf { expr: #lit.to_string() } }
            }
            ValidationData::Unique { case_insensitive } => quote! {
                ::snugom::types::ValidationRule::Unique { case_insensitive: #case_insensitive }
            },
            ValidationData::Custom { path_repr, .. } => {
                let lit = LitStr::new(path_repr, Span::call_site());
                quote! { ::snugom::types::ValidationRule::Custom { path: #lit.to_string() } }
            }
        };
        quote! {
            ::snugom::types::ValidationDescriptor { scope: #scope, rule: #rule }
        }
    }
}
