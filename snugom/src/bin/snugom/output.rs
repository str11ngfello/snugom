use anyhow::Result;
use clap::ValueEnum;
use colored::Colorize;
use comfy_table::{Attribute, Cell, Color as TableColor, Table};
use serde::Serialize;
use std::io::Write;

use crate::theme::{ICONS, THEME};

/// Output format options for CLI commands
#[derive(Clone, Debug, ValueEnum, Default, PartialEq)]
pub enum OutputFormat {
    /// Formatted table output (default)
    #[default]
    Table,
    /// JSON output for scripting
    Json,
    /// Compact single-line output
    Compact,
}

/// Global CLI options that affect output and behavior
#[derive(Clone, Debug)]
pub struct GlobalOptions {
    pub output_format: OutputFormat,
    pub quiet: bool,
    #[allow(dead_code)]
    pub verbose: bool,
    pub no_color: bool,
}

impl Default for GlobalOptions {
    fn default() -> Self {
        Self {
            output_format: OutputFormat::Table,
            quiet: false,
            verbose: false,
            no_color: false,
        }
    }
}

/// Trait for data that can be displayed as a table
#[allow(dead_code)]
pub trait TableDisplay {
    fn to_table(&self, options: &GlobalOptions) -> Table;
    fn to_compact(&self) -> String;
}

/// Output manager handles formatting and display
pub struct OutputManager {
    pub options: GlobalOptions,
}

impl OutputManager {
    pub fn new(options: GlobalOptions) -> Self {
        Self { options }
    }

    /// Display data according to the configured output format
    #[allow(dead_code)]
    pub fn display<T>(&self, data: &T) -> Result<()>
    where
        T: Serialize + TableDisplay,
    {
        if self.options.quiet {
            return Ok(());
        }

        match self.options.output_format {
            OutputFormat::Json => {
                let json = serde_json::to_string_pretty(data)?;
                println!("{json}");
            }
            OutputFormat::Table => {
                let table = data.to_table(&self.options);
                println!("{table}");
            }
            OutputFormat::Compact => {
                println!("{}", data.to_compact());
            }
        }
        Ok(())
    }

    /// Display a success message with color and icon
    pub fn success(&self, message: &str) {
        if !self.options.quiet {
            let output = if self.options.no_color {
                format!("{} {message}", ICONS.success)
            } else {
                format!(
                    "{} {}",
                    ICONS.success.color(THEME.success),
                    message.color(THEME.success)
                )
            };
            println!("{output}");
        }
    }

    /// Display an error message with color and icon
    pub fn error(&self, message: &str) {
        let output = if self.options.no_color {
            format!("{} {message}", ICONS.error)
        } else {
            format!(
                "{} {}",
                ICONS.error.color(THEME.error),
                message.color(THEME.error)
            )
        };
        eprintln!("{output}");
    }

    /// Display a warning message
    pub fn warning(&self, message: &str) {
        if !self.options.quiet {
            let output = if self.options.no_color {
                format!("{} {message}", ICONS.warning)
            } else {
                format!(
                    "{} {}",
                    ICONS.warning.color(THEME.warning),
                    message.color(THEME.warning)
                )
            };
            println!("{output}");
        }
    }

    /// Display verbose information (only if verbose mode is enabled)
    #[allow(dead_code)]
    pub fn verbose(&self, message: &str) {
        if self.options.verbose && !self.options.quiet {
            let output = if self.options.no_color {
                format!("{} {message}", ICONS.arrow)
            } else {
                format!(
                    "{} {}",
                    ICONS.arrow.color(THEME.muted),
                    message.color(THEME.muted)
                )
            };
            eprintln!("{output}");
        }
    }

    /// Display info message with color and icon
    pub fn info(&self, message: &str) {
        if !self.options.quiet {
            let output = if self.options.no_color {
                format!("{} {message}", ICONS.info)
            } else {
                format!(
                    "{} {}",
                    ICONS.info.color(THEME.info),
                    message.color(THEME.info)
                )
            };
            println!("{output}");
        }
    }

    /// Display a heading
    pub fn heading(&self, text: &str) {
        if !self.options.quiet {
            let output = if self.options.no_color {
                format!("\n{text}\n{}", "=".repeat(text.len()))
            } else {
                format!("\n{}", text.color(THEME.primary).bold())
            };
            println!("{output}");
        }
    }

    /// Display a subheading
    #[allow(dead_code)]
    pub fn subheading(&self, text: &str) {
        if !self.options.quiet {
            let output = if self.options.no_color {
                format!("\n{text}\n{}", "-".repeat(text.len()))
            } else {
                format!("\n{}", text.color(THEME.secondary).underline())
            };
            println!("{output}");
        }
    }

    /// Display a key-value pair
    #[allow(dead_code)]
    pub fn key_value(&self, key: &str, value: &str) {
        if !self.options.quiet {
            let output = if self.options.no_color {
                format!("{key}: {value}")
            } else {
                format!(
                    "{}: {}",
                    key.color(THEME.key).bold(),
                    value.color(THEME.value)
                )
            };
            println!("{output}");
        }
    }

    /// Display a bullet list item
    pub fn bullet(&self, text: &str) {
        if !self.options.quiet {
            let output = if self.options.no_color {
                format!("  {} {text}", ICONS.bullet)
            } else {
                format!("  {} {text}", ICONS.bullet.color(THEME.muted))
            };
            println!("{output}");
        }
    }

    /// Display indented text with a prefix icon
    pub fn indented(&self, icon: &str, text: &str) {
        if !self.options.quiet {
            let output = if self.options.no_color {
                format!("  {icon} {text}")
            } else {
                format!("  {} {text}", icon.color(THEME.muted))
            };
            println!("{output}");
        }
    }

    /// Create a themed table
    #[allow(dead_code)]
    pub fn create_table(&self) -> Table {
        let mut table = Table::new();

        if !self.options.no_color {
            table.load_preset(comfy_table::presets::UTF8_FULL_CONDENSED);
        } else {
            table.load_preset(comfy_table::presets::ASCII_FULL);
        }

        table
    }

    /// Add themed header to table
    #[allow(dead_code)]
    pub fn add_table_header(&self, table: &mut Table, headers: Vec<&str>) {
        let header_cells: Vec<Cell> = if self.options.no_color {
            headers
                .iter()
                .map(|h| Cell::new(h).add_attribute(Attribute::Bold))
                .collect()
        } else {
            headers
                .iter()
                .map(|h| {
                    Cell::new(h)
                        .add_attribute(Attribute::Bold)
                        .fg(TableColor::Cyan)
                })
                .collect()
        };
        table.set_header(header_cells);
    }

    /// Display progress indicator
    pub fn progress(&self, message: &str) {
        if self.options.quiet || matches!(self.options.output_format, OutputFormat::Json) {
            return;
        }

        let output = if self.options.no_color {
            format!("{} {message}...", ICONS.loading)
        } else {
            format!(
                "{} {}...",
                ICONS.loading.color(THEME.highlight).bold(),
                message.color(THEME.highlight)
            )
        };

        print!("\r{output}");
        std::io::stdout().flush().ok();
    }

    /// Clear the current line (useful after progress indicators)
    pub fn clear_line(&self) {
        if self.options.quiet || matches!(self.options.output_format, OutputFormat::Json) {
            return;
        }

        print!("\r{}", " ".repeat(80));
        print!("\r");
        std::io::stdout().flush().ok();
    }
}

/// Implementation for Vec<T> where T: TableDisplay
impl<T> TableDisplay for Vec<T>
where
    T: TableDisplay + Serialize,
{
    fn to_table(&self, options: &GlobalOptions) -> Table {
        let mut table = Table::new();

        if !options.no_color {
            table.load_preset(comfy_table::presets::UTF8_FULL_CONDENSED);
        }

        if self.is_empty() {
            table.add_row(vec![Cell::new("No items found")]);
            return table;
        }

        table.add_row(vec![Cell::new("Items").add_attribute(Attribute::Bold)]);
        for (i, item) in self.iter().enumerate() {
            table.add_row(vec![
                Cell::new(format!("{}", i + 1)),
                Cell::new(serde_json::to_string_pretty(item).unwrap_or_default()),
            ]);
        }

        table
    }

    fn to_compact(&self) -> String {
        format!("Count: {}", self.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct TestData {
        name: String,
        value: i32,
    }

    impl TableDisplay for TestData {
        fn to_table(&self, _options: &GlobalOptions) -> Table {
            let mut table = Table::new();
            table.add_row(vec![Cell::new("Name"), Cell::new(&self.name)]);
            table.add_row(vec![Cell::new("Value"), Cell::new(self.value.to_string())]);
            table
        }

        fn to_compact(&self) -> String {
            format!("{}={}", self.name, self.value)
        }
    }

    #[test]
    fn test_output_manager_json() {
        let options = GlobalOptions {
            output_format: OutputFormat::Json,
            ..Default::default()
        };
        let manager = OutputManager::new(options);
        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        assert!(manager.display(&data).is_ok());
    }

    #[test]
    fn test_output_manager_quiet() {
        let options = GlobalOptions {
            quiet: true,
            ..Default::default()
        };
        let manager = OutputManager::new(options);
        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        assert!(manager.display(&data).is_ok());
    }
}
