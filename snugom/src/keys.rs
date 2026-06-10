/// Common key-construction helpers used across SnugOM.
#[derive(Debug, Clone)]
pub struct KeyContext<'a> {
    pub prefix: &'a str,
}

impl<'a> KeyContext<'a> {
    pub fn new(prefix: &'a str) -> Self {
        Self { prefix }
    }

    pub fn entity(
        &self,
        collection: &str,
        entity_id: &str,
    ) -> String {
        format!("{}:{}:{}", self.prefix, collection, entity_id)
    }

    /// Returns a glob pattern matching all entities in a collection.
    /// Useful for test cleanup or batch operations.
    pub fn collection_pattern(
        &self,
        collection: &str,
    ) -> String {
        format!("{}:{}:*", self.prefix, collection)
    }

    pub fn relation(
        &self,
        alias: &str,
        left_id: &str,
    ) -> String {
        format!("{}:rel:{}:{}", self.prefix, alias, left_id)
    }

    pub fn relation_reverse(
        &self,
        alias: &str,
        right_id: &str,
    ) -> String {
        format!("{}:rel:{}_reverse:{}", self.prefix, alias, right_id)
    }

    /// Key for reverse relation lookup - finds all children of a given collection
    /// that have a belongs_to relation pointing to a specific parent entity.
    /// Format: prefix:child_collection:rev_rel:alias:parent_id
    pub fn reverse_relation(
        &self,
        child_collection: &str,
        alias: &str,
        parent_id: &str,
    ) -> String {
        format!(
            "{}:{}:rev_rel:{}:{}",
            self.prefix, child_collection, alias, parent_id
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_entity_keys() {
        let ctx = KeyContext::new("snug");
        assert_eq!(ctx.entity("users", "abc"), "snug:users:abc");
    }
}
