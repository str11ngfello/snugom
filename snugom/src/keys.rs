/// Common key-construction helpers used across SnugOM.
#[derive(Debug, Clone)]
pub struct KeyContext<'a> {
    pub prefix: &'a str,
    pub service: &'a str,
}

impl<'a> KeyContext<'a> {
    pub fn new(prefix: &'a str, service: &'a str) -> Self {
        Self { prefix, service }
    }

    pub fn entity(&self, collection: &str, entity_id: &str) -> String {
        format!("{}:{}:{}:{}", self.prefix, self.service, collection, entity_id)
    }

    /// Returns a glob pattern matching all entities in a collection.
    /// Useful for test cleanup or batch operations.
    pub fn collection_pattern(&self, collection: &str) -> String {
        format!("{}:{}:{}:*", self.prefix, self.service, collection)
    }

    /// Returns a glob pattern matching all keys in this service.
    /// Useful for test cleanup of all service data (entities + auxiliary keys).
    pub fn service_pattern(&self) -> String {
        format!("{}:{}:*", self.prefix, self.service)
    }

    pub fn relation(&self, alias: &str, left_id: &str) -> String {
        format!("{}:{}:rel:{}:{}", self.prefix, self.service, alias, left_id)
    }

    pub fn relation_reverse(&self, alias: &str, right_id: &str) -> String {
        format!(
            "{}:{}:rel:{}_reverse:{}",
            self.prefix, self.service, alias, right_id
        )
    }

    /// Key for reverse relation lookup - finds all children of a given collection
    /// that have a belongs_to relation pointing to a specific parent entity.
    /// Format: prefix:service:child_collection:rev_rel:alias:parent_id
    pub fn reverse_relation(&self, child_collection: &str, alias: &str, parent_id: &str) -> String {
        format!(
            "{}:{}:{}:rev_rel:{}:{}",
            self.prefix, self.service, child_collection, alias, parent_id
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_entity_keys() {
        let ctx = KeyContext::new("snug", "svc");
        assert_eq!(ctx.entity("users", "abc"), "snug:svc:users:abc");
    }
}
