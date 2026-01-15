use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::ValidationError;
use super::support;
use crate::{
    SnugomEntity,
    types::{EntityMetadata, ValidationDescriptor, ValidationRule, ValidationScope},
};

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "examples", collection = "articles")]
struct Article {
    #[snugom(id)]
    id: String,
    #[snugom(validate(length(min = 3, max = 12)))]
    title: String,
    #[snugom(validate(range(min = 0, max = 10)))]
    rating: i32,
    #[snugom(validate(regex = "^[a-z-]+$"))]
    slug: String,
    #[snugom(validate(length(min = 1)))]
    tags: Vec<String>,
    #[snugom(validate(length(min = 3)))]
    summary: Option<String>,
    #[allow(dead_code)]
    #[snugom(datetime, filterable, sortable)]
    published_at: Option<DateTime<Utc>>,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "examples", collection = "advanced")]
struct AdvancedEntity {
    #[snugom(id)]
    id: String,
    #[snugom(validate(email))]
    email: String,
    #[snugom(validate(url))]
    homepage: Option<String>,
    #[snugom(validate(uuid))]
    external_id: String,
    #[snugom(validate(enum(allowed = ["draft", "published"], case_insensitive = true)))]
    status: String,
    #[snugom(validate(each = "length(min = 2, max = 8)"))]
    labels: Vec<String>,
    #[snugom(validate(unique))]
    tags: Vec<String>,
    #[snugom(validate(required_if = "status == \"published\""))]
    #[snugom(datetime, filterable, sortable)]
    published_at: Option<DateTime<Utc>>,
    #[snugom(validate(forbidden_if = "status == \"published\""))]
    draft_reason: Option<String>,
    #[snugom(validate(custom = "crate::examples::repo::ex06_validation_rules::ensure_not_admin"))]
    slug: String,
}

pub fn ensure_not_admin(value: &String) -> ValidationResult<()> {
    if value == "admin" {
        Err(ValidationError::single("slug", "validation.custom", "slug cannot be admin"))
    } else {
        Ok(())
    }
}

type ValidationResult<T> = crate::errors::ValidationResult<T>;

/// Example 06 – validation DSL, descriptor metadata, and custom validators.
pub async fn run() -> Result<()> {
    // The validation story is purely in-memory; no Redis operations needed.
    support::unique_namespace("validation"); // keep entropy consistent for future expansions.

    let article = Article {
        id: "article-1".into(),
        title: "Welcome".into(),
        rating: 5,
        slug: "welcome-post".into(),
        tags: vec!["snugom".into()],
        summary: Some("great".into()),
        published_at: Some(Utc::now()),
    };
    assert!(article.validate().is_ok(), "valid article should pass");
    let mirrors = article.datetime_mirrors();
    assert_eq!(mirrors.len(), 1);
    assert_eq!(mirrors[0].mirror_field, "published_at_ts");
    assert!(mirrors[0].value.is_some());

    let invalid_article = Article {
        id: "article-2".into(),
        title: "hi".into(),
        rating: -1,
        slug: "Not Valid".into(),
        tags: Vec::new(),
        summary: Some("no".into()),
        published_at: None,
    };
    let err = invalid_article.validate().expect_err("expected aggregated errors");
    let fields: Vec<String> = err.issues.iter().map(|issue| issue.field.clone()).collect();
    assert!(fields.contains(&"title".into()));
    assert!(fields.contains(&"rating".into()));
    assert!(fields.contains(&"slug".into()));
    assert!(fields.contains(&"tags".into()));
    assert!(fields.contains(&"summary".into()));
    let mirrors = invalid_article.datetime_mirrors();
    assert_eq!(mirrors.len(), 1);
    assert!(mirrors[0].value.is_none());

    let descriptor = Article::entity_descriptor();
    let title = descriptor
        .fields
        .iter()
        .find(|field| field.name == "title")
        .expect("title field present");
    assert!(matches!(
        title.validations[0],
        ValidationDescriptor {
            scope: ValidationScope::Field,
            rule: ValidationRule::Length { .. }
        }
    ));

    let advanced_ok = AdvancedEntity {
        id: "advanced-1".into(),
        email: "user@example.com".into(),
        homepage: Some("https://example.com".into()),
        external_id: "550e8400-e29b-41d4-a716-446655440000".into(),
        status: "Published".into(),
        labels: vec!["ok".into(), "good".into()],
        tags: vec!["one".into(), "two".into()],
        published_at: Some(Utc::now()),
        draft_reason: None,
        slug: "safe".into(),
    };
    assert!(advanced_ok.validate().is_ok());

    let advanced_fail = AdvancedEntity {
        id: "advanced-2".into(),
        email: "invalid".into(),
        homepage: Some("not-a-url".into()),
        external_id: "nope".into(),
        status: "published".into(),
        labels: vec!["a".into(), "toolongvalue".into()],
        tags: vec!["dup".into(), "dup".into()],
        published_at: None,
        draft_reason: Some("should be blank".into()),
        slug: "admin".into(),
    };
    let err = advanced_fail.validate().expect_err("validation should fail");
    let mut advanced_fields: Vec<String> = err.issues.iter().map(|issue| issue.field.clone()).collect();
    advanced_fields.sort();
    assert!(advanced_fields.contains(&"email".into()));
    assert!(advanced_fields.contains(&"homepage".into()));
    assert!(advanced_fields.contains(&"external_id".into()));
    assert!(advanced_fields.contains(&"labels[0]".into()));
    assert!(advanced_fields.contains(&"labels[1]".into()));
    // Note: Vec-level unique validation (checking for duplicate elements) is enforced
    // at database level via Lua script, not in local validation. See validation_emit.rs.
    assert!(advanced_fields.contains(&"published_at".into()));
    assert!(advanced_fields.contains(&"draft_reason".into()));
    assert!(advanced_fields.contains(&"slug".into()));

    Ok(())
}
