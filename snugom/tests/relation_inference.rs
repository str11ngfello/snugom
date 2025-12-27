//! Tests for field-based relation inference
//!
//! This demonstrates the new ergonomic syntax for declaring relations using
//! field attributes instead of container-level attributes.

use serde::{Deserialize, Serialize};
use snugom::{bundle, types::EntityMetadata, SnugomEntity};

/// Example using belongs_to inference from {entity}_id fields
mod belongs_to_inference {
    use super::*;

    #[derive(SnugomEntity, Serialize, Deserialize)]
    #[snugom(version = 1)]
    pub struct Organization {
        #[snugom(id)]
        pub id: String,
        #[snugom(filterable(tag))]
        pub name: String,
    }

    #[derive(SnugomEntity, Serialize, Deserialize)]
    #[snugom(version = 1)]
    pub struct Team {
        #[snugom(id)]
        pub id: String,
        #[snugom(filterable(tag))]
        pub name: String,

        // belongs_to inferred from {entity}_id pattern
        #[snugom(relation, filterable(tag))]
        pub organization_id: String,
    }

    #[derive(SnugomEntity, Serialize, Deserialize)]
    #[snugom(version = 1)]
    pub struct Employee {
        #[snugom(id)]
        pub id: String,
        #[snugom(filterable(tag))]
        pub name: String,

        // belongs_to inferred from team_id field
        #[snugom(relation, filterable(tag))]
        pub team_id: String,

        // Additional foreign key without relation (not all _id fields are relations)
        pub department_id: String,
    }

    bundle! {
        service: "ri",
        entities: {
            Organization => "organizations",
            Team => "teams",
            Employee => "employees",
        }
    }
}

/// Example with explicit cascade policy on belongs_to
mod cascade_policy {
    use super::*;

    #[derive(SnugomEntity, Serialize, Deserialize)]
    #[snugom(version = 1)]
    pub struct Parent {
        #[snugom(id)]
        pub id: String,
        #[snugom(filterable(tag))]
        pub name: String,
    }

    #[derive(SnugomEntity, Serialize, Deserialize)]
    #[snugom(version = 1)]
    pub struct Child {
        #[snugom(id)]
        pub id: String,
        #[snugom(filterable(tag))]
        pub value: String,

        // belongs_to with explicit cascade policy
        #[snugom(relation(cascade = "delete"), filterable(tag))]
        pub parent_id: String,
    }

    bundle! {
        service: "cp",
        entities: {
            Parent => "parents",
            Child => "children",
        }
    }
}


/// Example with explicit target and alias
mod explicit_target {
    use super::*;

    #[derive(SnugomEntity, Serialize, Deserialize)]
    #[snugom(version = 1)]
    pub struct Author {
        #[snugom(id)]
        pub id: String,
        #[snugom(filterable(tag))]
        pub name: String,
    }

    #[derive(SnugomEntity, Serialize, Deserialize)]
    #[snugom(version = 1)]
    pub struct Book {
        #[snugom(id)]
        pub id: String,
        #[snugom(filterable(tag))]
        pub title: String,

        // Explicit target and alias when naming doesn't follow convention
        #[snugom(relation(target = "authors", alias = "written_by"), filterable(tag))]
        pub author_id: String,
    }

    bundle! {
        service: "et",
        entities: {
            Author => "authors",
            Book => "books",
        }
    }
}

#[test]
fn test_belongs_to_inferred_from_field_name() {
    let descriptor = belongs_to_inference::Team::entity_descriptor();

    // Should have a belongs_to relation to organizations
    let org_rel = descriptor.relations.iter()
        .find(|r| r.alias == "organization")
        .expect("should have organization relation");

    assert_eq!(org_rel.target, "organizations");
    assert!(matches!(org_rel.kind, snugom::types::RelationKind::BelongsTo));
    assert_eq!(org_rel.foreign_key, Some("organization_id".to_string()));
}

#[test]
fn test_multiple_belongs_to_relations() {
    let emp_desc = belongs_to_inference::Employee::entity_descriptor();

    // Should have team relation (from #[snugom(relation)] on team_id)
    let team_rel = emp_desc.relations.iter()
        .find(|r| r.alias == "team")
        .expect("should have team relation");

    assert_eq!(team_rel.target, "teams");
    assert!(matches!(team_rel.kind, snugom::types::RelationKind::BelongsTo));
    assert_eq!(team_rel.foreign_key, Some("team_id".to_string()));

    // Should NOT have department relation (department_id doesn't have #[snugom(relation)])
    let dept_rel = emp_desc.relations.iter()
        .find(|r| r.alias == "department");
    assert!(dept_rel.is_none(), "department_id without #[snugom(relation)] should not create relation");
}

#[test]
fn test_cascade_policy_from_field() {
    let child_desc = cascade_policy::Child::entity_descriptor();

    let parent_rel = child_desc.relations.iter()
        .find(|r| r.alias == "parent")
        .expect("should have parent relation");

    // Should have cascade delete
    assert!(matches!(parent_rel.cascade, snugom::types::CascadePolicy::Delete));
}


#[test]
fn test_explicit_target_and_alias() {
    let book_desc = explicit_target::Book::entity_descriptor();

    let author_rel = book_desc.relations.iter()
        .find(|r| r.alias == "written_by")
        .expect("should have written_by relation");

    assert_eq!(author_rel.target, "authors");
    assert_eq!(author_rel.foreign_key, Some("author_id".to_string()));
}

#[test]
fn test_default_cascade_is_none() {
    let team_desc = belongs_to_inference::Team::entity_descriptor();

    let org_rel = team_desc.relations.iter()
        .find(|r| r.alias == "organization")
        .expect("should have organization relation");

    // Default cascade should be None
    assert!(matches!(org_rel.cascade, snugom::types::CascadePolicy::None));
}
