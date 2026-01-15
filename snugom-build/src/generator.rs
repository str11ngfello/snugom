//! Code generator for SnugomClient.

use crate::scanner::{EntityInfo, scan_directory};
use anyhow::{Context, Result};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Builder for configuring and running the SnugomClient generator.
pub struct ClientGenerator {
    scan_paths: Vec<PathBuf>,
    output_file: PathBuf,
    crate_name: String,
    client_name: String,
}

impl ClientGenerator {
    /// Create a new generator with default settings.
    pub fn new() -> Self {
        Self {
            scan_paths: Vec::new(),
            output_file: PathBuf::from("src/generated/snugom_client.rs"),
            crate_name: "crate".to_string(),
            client_name: "SnugomClient".to_string(),
        }
    }

    /// Add a path to scan for entity definitions.
    ///
    /// Can be called multiple times to scan multiple directories.
    pub fn scan_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.scan_paths.push(path.into());
        self
    }

    /// Set the output file path for the generated code.
    ///
    /// Default: `src/generated/snugom_client.rs`
    pub fn output_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.output_file = path.into();
        self
    }

    /// Set the crate name used in import paths.
    ///
    /// Default: `crate`
    pub fn crate_name(mut self, name: impl Into<String>) -> Self {
        self.crate_name = name.into();
        self
    }

    /// Set the name of the generated client struct.
    ///
    /// Default: `SnugomClient`
    pub fn client_name(mut self, name: impl Into<String>) -> Self {
        self.client_name = name.into();
        self
    }

    /// Run the generator.
    ///
    /// This scans all configured paths, discovers entities, and writes
    /// the generated SnugomClient to the output file.
    pub fn run(self) -> Result<()> {
        // Default to scanning "src/" if no paths specified
        let scan_paths = if self.scan_paths.is_empty() {
            vec![PathBuf::from("src/")]
        } else {
            self.scan_paths
        };

        // Discover all entities
        let mut all_entities = Vec::new();
        for path in &scan_paths {
            let entities =
                scan_directory(path, &self.crate_name).with_context(|| format!("Failed to scan {}", path.display()))?;
            all_entities.extend(entities);
        }

        // Deduplicate by name (in case same entity found multiple times)
        let mut seen = std::collections::HashSet::new();
        all_entities.retain(|e| seen.insert(e.name.clone()));

        // Sort for deterministic output
        all_entities.sort_by(|a, b| a.name.cmp(&b.name));

        // Generate the code
        let code = generate_client_code(&self.client_name, &all_entities)?;

        // Ensure output directory exists
        if let Some(parent) = self.output_file.parent() {
            fs::create_dir_all(parent).with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }

        // Only write if content has changed (avoids unnecessary recompilation)
        let should_write = match fs::read_to_string(&self.output_file) {
            Ok(existing) => existing != code,
            Err(_) => true, // File doesn't exist, need to write
        };

        if should_write {
            fs::write(&self.output_file, &code)
                .with_context(|| format!("Failed to write {}", self.output_file.display()))?;
            eprintln!(
                "snugom-build: Generated {} with {} entities",
                self.output_file.display(),
                all_entities.len()
            );
        }

        // Generate mod.rs to expose the client module
        if let Some(parent) = self.output_file.parent() {
            let mod_file = parent.join("mod.rs");
            let file_stem = self
                .output_file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("snugom_client");

            let mod_content = format!(
                "//! Auto-generated module. Do not edit manually.\n\npub mod {file_stem};\npub use {file_stem}::*;\n"
            );

            let should_write_mod = match fs::read_to_string(&mod_file) {
                Ok(existing) => existing != mod_content,
                Err(_) => true,
            };

            if should_write_mod {
                fs::write(&mod_file, &mod_content)
                    .with_context(|| format!("Failed to write {}", mod_file.display()))?;
                eprintln!("snugom-build: Generated {}", mod_file.display());
            }
        }

        Ok(())
    }
}

impl Default for ClientGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate the SnugomClient code.
fn generate_client_code(client_name: &str, entities: &[EntityInfo]) -> Result<String> {
    let client_ident = format_ident!("{}", client_name);

    // Group entities by module path for imports
    let mut imports_by_module: HashMap<String, Vec<String>> = HashMap::new();
    for entity in entities {
        imports_by_module
            .entry(entity.module_path.clone())
            .or_default()
            .push(entity.name.clone());
    }

    // Generate import statements
    let imports: Vec<TokenStream> = imports_by_module
        .iter()
        .map(|(module, names)| {
            let module_path: syn::Path = syn::parse_str(module).unwrap();
            let name_idents: Vec<_> = names.iter().map(|n| format_ident!("{}", n)).collect();
            quote! {
                use #module_path::{#(#name_idents),*};
            }
        })
        .collect();

    // Generate accessor methods
    let accessors: Vec<TokenStream> = entities
        .iter()
        .map(|entity| {
            let entity_ident = format_ident!("{}", entity.name);
            let method_name = format_ident!("{}", pluralize(&to_snake_case(&entity.name)));

            quote! {
                /// Get a collection handle for [`#entity_ident`] entities.
                pub fn #method_name(&self) -> ::snugom::CollectionHandle<#entity_ident> {
                    let repo = ::snugom::Repo::new(self.prefix.clone());
                    ::snugom::CollectionHandle::new(repo, self.conn.clone())
                }
            }
        })
        .collect();

    // Generate ensure_registered calls
    let ensure_registered_calls: Vec<TokenStream> = entities
        .iter()
        .map(|entity| {
            let entity_ident = format_ident!("{}", entity.name);
            quote! {
                <#entity_ident as ::snugom::types::EntityMetadata>::ensure_registered();
            }
        })
        .collect();

    // Generate ensure_index calls
    let ensure_index_calls: Vec<TokenStream> = entities
        .iter()
        .map(|entity| {
            let entity_ident = format_ident!("{}", entity.name);
            quote! {
                {
                    use ::snugom::search::SearchEntity;
                    let definition = <#entity_ident as SearchEntity>::index_definition(&self.prefix);
                    ::snugom::search::ensure_index(&mut self.conn, &definition).await?;
                }
            }
        })
        .collect();

    // Generate the full module
    let output = quote! {
        //! Auto-generated SnugomClient. Do not edit manually.
        //!
        //! Regenerate with: `cargo build`
        //!
        //! Generated by snugom-build.

        #![allow(unused_imports)]

        use ::redis::aio::ConnectionManager;

        #(#imports)*

        /// Auto-generated client with typed accessors for all SnugomEntity types.
        #[derive(Clone)]
        pub struct #client_ident {
            conn: ConnectionManager,
            prefix: String,
        }

        impl #client_ident {
            /// Connect to Redis and initialize all indexes.
            ///
            /// This is the recommended way to create a client. It automatically
            /// calls `ensure_indexes()` after connecting.
            pub async fn connect(
                url: &str,
                prefix: impl Into<String>,
            ) -> Result<Self, ::snugom::errors::RepoError> {
                let redis_client = ::redis::Client::open(url)?;
                let conn = ConnectionManager::new(redis_client).await?;
                Self::from_connection(conn, prefix).await
            }

            /// Create a client from an existing Redis connection.
            ///
            /// This automatically calls `ensure_indexes()` to initialize search indexes.
            /// Use this when you have an existing ConnectionManager you want to reuse.
            pub async fn from_connection(
                conn: ConnectionManager,
                prefix: impl Into<String>,
            ) -> Result<Self, ::snugom::errors::RepoError> {
                let mut client = Self {
                    conn,
                    prefix: prefix.into(),
                };
                client.ensure_indexes().await?;
                Ok(client)
            }

            /// Ensure all search indexes exist for registered entities.
            ///
            /// This is called automatically by [`connect`] and [`from_connection`].
            /// You typically don't need to call this manually.
            pub async fn ensure_indexes(&mut self) -> Result<(), ::snugom::errors::RepoError> {
                // Register all entity descriptors
                #(#ensure_registered_calls)*

                // Create search indexes
                #(#ensure_index_calls)*

                Ok(())
            }

            /// Get a clone of the connection manager.
            pub fn connection(&self) -> ConnectionManager {
                self.conn.clone()
            }

            /// Get a mutable reference to the connection manager.
            pub fn connection_mut(&mut self) -> &mut ConnectionManager {
                &mut self.conn
            }

            /// Get the key prefix.
            pub fn prefix(&self) -> &str {
                &self.prefix
            }

            /// Get a generic collection handle for any entity type.
            pub fn collection<E: ::snugom::types::SnugomModel>(&self) -> ::snugom::CollectionHandle<E> {
                let repo = ::snugom::Repo::new(self.prefix.clone());
                ::snugom::CollectionHandle::new(repo, self.conn.clone())
            }

            // ============ Entity Accessors ============

            #(#accessors)*
        }
    };

    // Format with prettyplease for readable output
    let syntax_tree = syn::parse2(output).context("Failed to parse generated code")?;
    Ok(prettyplease::unparse(&syntax_tree))
}

/// Convert PascalCase to snake_case.
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

/// Simple pluralization.
fn pluralize(word: &str) -> String {
    if word.ends_with('s') || word.ends_with('x') || word.ends_with("ch") || word.ends_with("sh") {
        format!("{word}es")
    } else if word.ends_with('y')
        && !word.ends_with("ay")
        && !word.ends_with("ey")
        && !word.ends_with("oy")
        && !word.ends_with("uy")
    {
        format!("{}ies", &word[..word.len() - 1])
    } else {
        format!("{word}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("Guild"), "guild");
        assert_eq!(to_snake_case("GuildMember"), "guild_member");
        assert_eq!(to_snake_case("HTTPRequest"), "h_t_t_p_request");
    }

    #[test]
    fn test_pluralize() {
        assert_eq!(pluralize("guild"), "guilds");
        assert_eq!(pluralize("match"), "matches");
        assert_eq!(pluralize("category"), "categories");
        assert_eq!(pluralize("key"), "keys");
    }
}
