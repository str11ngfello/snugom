use nanoid::nanoid;

/// Canonical alphabet for SnugOM entity identifiers (no ambiguous glyphs).
const ENTITY_ID_ALPHABET: &[char] = &[
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'J', 'K', 'L', 'M', 'N', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y',
    'Z', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'j', 'm', 'n', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
];
/// Default entity id length.
const ENTITY_ID_LENGTH: usize = 20;

/// Generates a new entity identifier using the configured alphabet and length.
pub fn generate_entity_id() -> String {
    nanoid!(ENTITY_ID_LENGTH, ENTITY_ID_ALPHABET)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_has_expected_length_and_charset() {
        let id = generate_entity_id();
        assert_eq!(id.len(), ENTITY_ID_LENGTH);
        assert!(id.chars().all(|c| ENTITY_ID_ALPHABET.contains(&c)));
    }
}
