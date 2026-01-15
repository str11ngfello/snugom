use chrono::{DateTime, Utc};

/// Format a Unix timestamp to a human-readable string
#[allow(dead_code)]
pub fn format_timestamp(ts: u64) -> String {
    let dt = DateTime::<Utc>::from_timestamp(ts as i64, 0).unwrap_or_else(|| {
        DateTime::<Utc>::from_timestamp(0, 0).expect("Epoch timestamp should be valid")
    });
    dt.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

/// Format a DateTime to a human-readable string
#[allow(dead_code)]
pub fn format_datetime(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

/// Format file size in human-readable format
#[allow(dead_code)]
pub fn format_file_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];

    if size == 0 {
        return "0 B".to_string();
    }

    let mut size_f = size as f64;
    let mut unit_index = 0;

    while size_f >= 1024.0 && unit_index < UNITS.len() - 1 {
        size_f /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{size} {}", UNITS[unit_index])
    } else {
        format!("{size_f:.2} {}", UNITS[unit_index])
    }
}

/// Generate a migration filename with timestamp
#[allow(dead_code)]
pub fn migration_filename(name: &str) -> String {
    let now = Utc::now();
    let timestamp = now.format("%Y%m%d_%H%M%S");
    format!("{timestamp}_{name}.rs")
}

/// Sanitize a name for use as a Rust identifier
#[allow(dead_code)]
pub fn sanitize_identifier(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_file_size() {
        assert_eq!(format_file_size(0), "0 B");
        assert_eq!(format_file_size(100), "100 B");
        assert_eq!(format_file_size(1024), "1.00 KB");
        assert_eq!(format_file_size(1024 * 1024), "1.00 MB");
    }

    #[test]
    fn test_sanitize_identifier() {
        assert_eq!(sanitize_identifier("add_user"), "add_user");
        assert_eq!(sanitize_identifier("add-user"), "add_user");
        assert_eq!(sanitize_identifier("add user"), "add_user");
        assert_eq!(sanitize_identifier("123_test"), "123_test");
    }

    #[test]
    fn test_migration_filename() {
        let filename = migration_filename("add_avatar");
        assert!(filename.ends_with("_add_avatar.rs"));
        assert!(filename.len() > 20); // Has timestamp prefix
    }
}
