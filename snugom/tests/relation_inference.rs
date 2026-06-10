//! Tests for field-based relation declaration.
//!
//! Canonical convention: a belongs_to relation requires an explicit `target`
//! collection (no name-based pluralization inference). The relation alias
//! defaults to the foreign-key field name and can be overridden with `alias`.

use serde::{Deserialize, Serialize};
use snugom::{SnugomEntity, types::EntityMetadata};

/// belongs_to with explicit target; alias defaults to the field name.
mod belongs_to_basic {
    use super::*;

    #[derive(SnugomEntity, Serialize, Deserialize)]
    #[snugom(schema = 1, collection = "organizations")]
    pub struct Organization {
        #[snugom(id)]
        pub id: String,
        #[snugom(filterable(tag))]
        pub name: String,
    }

    #[derive(SnugomEntity, Serialize, Deserialize)]
    #[snugom(schema = 1, collection = "teams")]
    pub struct Team {
        #[snugom(id)]
        pub id: String,
        #[snugom(filterable(tag))]
        pub name: String,

        // belongs_to: explicit target required; alias defaults to "organization_id".
        #[snugom(relation(target = "organizations"), filterable(tag))]
        pub organization_id: String,
    }

    #[derive(SnugomEntity, Serialize, Deserialize)]
    #[snugom(schema = 1, collection = "employees")]
    pub struct Employee {
        #[snugom(id)]
        pub id: String,
        #[snugom(filterable(tag))]
        pub name: String,

        #[snugom(relation(target = "teams"), filterable(tag))]
        pub team_id: String,

        // A foreign key without #[snugom(relation)] is NOT a relation.
        pub department_id: String,
    }
}

/// belongs_to with explicit cascade policy.
mod cascade_policy {
    use super::*;

    #[derive(SnugomEntity, Serialize, Deserialize)]
    #[snugom(schema = 1, collection = "parents")]
    pub struct Parent {
        #[snugom(id)]
        pub id: String,
        #[snugom(filterable(tag))]
        pub name: String,
    }

    #[derive(SnugomEntity, Serialize, Deserialize)]
    #[snugom(schema = 1, collection = "children")]
    pub struct Child {
        #[snugom(id)]
        pub id: String,
        #[snugom(filterable(tag))]
        pub value: String,

        #[snugom(relation(target = "parents", cascade = "delete"), filterable(tag))]
        pub parent_id: String,
    }
}

/// Explicit alias override (e.g. when the field name isn't the desired alias).
mod explicit_alias {
    use super::*;

    #[derive(SnugomEntity, Serialize, Deserialize)]
    #[snugom(schema = 1, collection = "authors")]
    pub struct Author {
        #[snugom(id)]
        pub id: String,
        #[snugom(filterable(tag))]
        pub name: String,
    }

    #[derive(SnugomEntity, Serialize, Deserialize)]
    #[snugom(schema = 1, collection = "books")]
    pub struct Book {
        #[snugom(id)]
        pub id: String,
        #[snugom(filterable(tag))]
        pub title: String,

        #[snugom(relation(target = "authors", alias = "written_by"), filterable(tag))]
        pub author_id: String,
    }
}

#[test]
fn belongs_to_uses_explicit_target_and_stripped_alias() {
    let descriptor = belongs_to_basic::Team::entity_descriptor();

    let org_rel = descriptor
        .relations
        .iter()
        .find(|r| r.alias == "organization")
        .expect("alias defaults to the field name with a trailing _id stripped");

    assert_eq!(org_rel.target, "organizations");
    assert!(matches!(org_rel.kind, snugom::types::RelationKind::BelongsTo));
    assert_eq!(org_rel.foreign_key, Some("organization_id".to_string()));
}

#[test]
fn only_annotated_foreign_keys_become_relations() {
    let emp_desc = belongs_to_basic::Employee::entity_descriptor();

    let team_rel = emp_desc
        .relations
        .iter()
        .find(|r| r.alias == "team")
        .expect("should have team relation");
    assert_eq!(team_rel.target, "teams");
    assert!(matches!(team_rel.kind, snugom::types::RelationKind::BelongsTo));
    assert_eq!(team_rel.foreign_key, Some("team_id".to_string()));

    // department_id has no #[snugom(relation)] -> not a relation.
    assert!(
        emp_desc.relations.iter().all(|r| r.alias != "department"),
        "an unannotated foreign key must not create a relation"
    );
}

#[test]
fn cascade_policy_is_read_from_the_attribute() {
    let child_desc = cascade_policy::Child::entity_descriptor();

    let parent_rel = child_desc
        .relations
        .iter()
        .find(|r| r.alias == "parent")
        .expect("should have parent relation");

    assert!(matches!(parent_rel.cascade, snugom::types::CascadePolicy::Delete));
}

#[test]
fn alias_can_be_overridden() {
    let book_desc = explicit_alias::Book::entity_descriptor();

    let author_rel = book_desc
        .relations
        .iter()
        .find(|r| r.alias == "written_by")
        .expect("explicit alias overrides the field-name default");

    assert_eq!(author_rel.target, "authors");
    assert_eq!(author_rel.foreign_key, Some("author_id".to_string()));
}

#[test]
fn default_cascade_is_none() {
    let team_desc = belongs_to_basic::Team::entity_descriptor();

    let org_rel = team_desc
        .relations
        .iter()
        .find(|r| r.alias == "organization")
        .expect("should have organization relation");

    assert!(matches!(org_rel.cascade, snugom::types::CascadePolicy::None));
}
