use email_address::EmailAddress;
use url::Url;
use uuid::Uuid;

/// Returns `true` if the provided string is a syntactically valid email address.
pub fn is_valid_email(value: &str) -> bool {
    EmailAddress::is_valid(value)
}

/// Returns `true` if the provided string parses as a URL with a scheme.
pub fn is_valid_url(value: &str) -> bool {
    Url::parse(value).is_ok()
}

/// Returns `true` if the provided string parses as a UUID.
pub fn is_valid_uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_validation() {
        assert!(is_valid_email("test@example.com"));
        assert!(!is_valid_email("invalid"));
    }

    #[test]
    fn url_validation() {
        assert!(is_valid_url("https://example.com"));
        assert!(!is_valid_url("not-a-url"));
    }

    #[test]
    fn uuid_validation() {
        assert!(is_valid_uuid("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!is_valid_uuid("not-a-uuid"));
    }
}
