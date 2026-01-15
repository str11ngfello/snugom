use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Project context for snugom operations
pub struct ProjectContext {
    /// Root directory of the project (where Cargo.toml is)
    pub project_root: PathBuf,
    /// Path to .snugom directory
    pub snugom_dir: PathBuf,
    /// Path to config file
    pub config_path: PathBuf,
    /// Path to schemas directory
    pub schemas_dir: PathBuf,
    /// Path to migrations directory
    pub migrations_dir: PathBuf,
    /// Loaded configuration
    pub config: Option<SnugomConfig>,
}

/// Configuration stored in .snugom/config.toml
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SnugomConfig {
    #[serde(default)]
    pub snugom: SnugomSettings,
    #[serde(default)]
    pub redis: RedisSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnugomSettings {
    #[serde(default = "default_migrations_dir")]
    pub migrations_dir: String,
    #[serde(default = "default_schemas_dir")]
    pub schemas_dir: String,
}

impl Default for SnugomSettings {
    fn default() -> Self {
        Self {
            migrations_dir: default_migrations_dir(),
            schemas_dir: default_schemas_dir(),
        }
    }
}

fn default_migrations_dir() -> String {
    "src/migrations".to_string()
}

fn default_schemas_dir() -> String {
    ".snugom/schemas".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisSettings {
    #[serde(default = "default_redis_url")]
    pub url: String,
}

impl Default for RedisSettings {
    fn default() -> Self {
        Self {
            url: default_redis_url(),
        }
    }
}

fn default_redis_url() -> String {
    "${REDIS_URL}".to_string()
}

impl ProjectContext {
    /// Find and load project context from current directory or ancestors
    pub fn find() -> Result<Self> {
        let current_dir = std::env::current_dir().context("Failed to get current directory")?;
        Self::find_from(&current_dir)
    }

    /// Find project context starting from the given directory
    pub fn find_from(start: &Path) -> Result<Self> {
        let project_root = Self::find_project_root(start)?;
        Self::from_root(project_root)
    }

    /// Create context from a known project root
    pub fn from_root(project_root: PathBuf) -> Result<Self> {
        let snugom_dir = project_root.join(".snugom");
        let config_path = snugom_dir.join("config.toml");

        // Load config if it exists
        let config = if config_path.exists() {
            let content =
                std::fs::read_to_string(&config_path).context("Failed to read config.toml")?;
            let config: SnugomConfig =
                toml::from_str(&content).context("Failed to parse config.toml")?;
            Some(config)
        } else {
            None
        };

        // Determine paths based on config or defaults
        let (schemas_dir, migrations_dir) = if let Some(ref cfg) = config {
            (
                project_root.join(&cfg.snugom.schemas_dir),
                project_root.join(&cfg.snugom.migrations_dir),
            )
        } else {
            (snugom_dir.join("schemas"), project_root.join("src/migrations"))
        };

        Ok(Self {
            project_root,
            snugom_dir,
            config_path,
            schemas_dir,
            migrations_dir,
            config,
        })
    }

    /// Find project root by looking for Cargo.toml
    fn find_project_root(start: &Path) -> Result<PathBuf> {
        let mut current = start.to_path_buf();

        loop {
            let cargo_toml = current.join("Cargo.toml");
            if cargo_toml.exists() {
                return Ok(current);
            }

            if !current.pop() {
                anyhow::bail!(
                    "Could not find Cargo.toml in {start:?} or any parent directory. \
                     Are you in a Rust project?"
                );
            }
        }
    }

    /// Check if snugom is initialized in this project
    pub fn is_initialized(&self) -> bool {
        self.snugom_dir.exists() && self.config_path.exists()
    }

    /// Get the Redis URL, expanding environment variables
    pub fn redis_url(&self) -> Result<String> {
        let url = self
            .config
            .as_ref()
            .map(|c| c.redis.url.as_str())
            .unwrap_or("${REDIS_URL}");

        // Expand environment variables
        if url.starts_with("${") && url.ends_with('}') {
            let var_name = &url[2..url.len() - 1];
            std::env::var(var_name)
                .with_context(|| format!("Environment variable {var_name} not set"))
        } else {
            Ok(url.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SnugomConfig::default();
        assert_eq!(config.snugom.migrations_dir, "src/migrations");
        assert_eq!(config.snugom.schemas_dir, ".snugom/schemas");
        assert_eq!(config.redis.url, "${REDIS_URL}");
    }

    #[test]
    fn test_config_serialization() {
        let config = SnugomConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("migrations_dir"));
        assert!(toml_str.contains("schemas_dir"));
    }
}
