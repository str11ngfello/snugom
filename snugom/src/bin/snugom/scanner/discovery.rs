//! File discovery for finding Rust files containing SnugomEntity.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Discovered entity file with basic metadata
#[derive(Debug, Clone)]
pub struct DiscoveredFile {
    /// Absolute path to the file
    pub path: PathBuf,
    /// Path relative to project root
    pub relative_path: String,
}

/// Discover all Rust files that might contain SnugomEntity derives.
///
/// Walks the source directories and finds .rs files that contain
/// the string "SnugomEntity" (either as derive or import).
pub fn discover_entities(project_root: &Path) -> Result<Vec<DiscoveredFile>> {
    let mut discovered = Vec::new();

    // Directories to search
    let search_dirs = ["src", "tests", "examples"];

    for dir in &search_dirs {
        let search_path = project_root.join(dir);
        if !search_path.exists() {
            continue;
        }

        for entry in WalkDir::new(&search_path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Skip non-Rust files
            if path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }

            // Skip hidden files and directories
            if path
                .components()
                .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
            {
                continue;
            }

            // Check if file contains SnugomEntity
            if file_contains_snugom_entity(path)? {
                let relative_path = path
                    .strip_prefix(project_root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .to_string();

                discovered.push(DiscoveredFile {
                    path: path.to_path_buf(),
                    relative_path,
                });
            }
        }
    }

    // Sort by relative path for consistent ordering
    discovered.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

    Ok(discovered)
}

/// Quick check if a file contains SnugomEntity derive.
///
/// This is a fast text-based check before doing full parsing.
fn file_contains_snugom_entity(path: &Path) -> Result<bool> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    // Look for derive(SnugomEntity) or derive(..., SnugomEntity, ...)
    Ok(content.contains("SnugomEntity"))
}
