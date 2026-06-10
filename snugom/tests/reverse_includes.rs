//! Integration tests for reverse includes — resolving a parent's children via
//! incoming `belongs_to` relations discovered in the registry (no relation
//! declared on the parent), reusing the FT.SEARCH-by-foreign-key path.
//!
//! Covers: returns children, cross-parent isolation, empty, options
//! (limit/sort/offset + metadata), multi-FK disambiguation, unknown-skip,
//! mixed forward+reverse, and the typed `include::<T>()` accessor.

mod common;

use std::collections::HashMap;

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use snugom::{SnugomClient, SnugomEntity, errors::RepoError, types::RelationQueryOptions};

// ============ Entities ============
// Parent stays lean (no child fields beyond the forward `posts` has_many used
// by the mixed test). Children declare belongs_to -> rev_users via a FK field;
// the FK is auto-tag-indexed by the macro, which reverse includes require.

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, collection = "rev_users")]
struct RevUser {
    #[snugom(id)]
    id: String,
    #[snugom(filterable(tag))]
    email: String,
    /// Forward has_many over the relation set (no back-FK on the child); read via
    /// the `posts` include's Lua set path.
    #[serde(default, skip_serializing)]
    #[snugom(relation(target = "rev_posts"))]
    #[allow(dead_code)]
    posts: Vec<String>,
    /// Forward has_many to a child that DOES declare belongs_to rev_users, so
    /// `?include=memberships(limit:..)` resolves via the FT.SEARCH-by-FK path.
    #[serde(default, skip_serializing)]
    #[snugom(relation(target = "rev_memberships"))]
    #[allow(dead_code)]
    memberships: Vec<String>,
}

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, collection = "rev_posts")]
struct RevPost {
    #[snugom(id)]
    id: String,
    #[snugom(filterable(text))]
    title: String,
}

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, collection = "rev_memberships")]
struct RevMembership {
    #[snugom(id)]
    id: String,
    #[snugom(relation(target = "rev_users"))]
    user_id: String,
    #[snugom(filterable(tag))]
    role: String,
    #[snugom(datetime, filterable, sortable)]
    joined_at: chrono::DateTime<Utc>,
}

/// Child with TWO foreign keys to the same parent — drives disambiguation.
/// Aliases derive from the field name minus `_id`: "sender" and "recipient".
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, collection = "rev_messages")]
struct RevMessage {
    #[snugom(id)]
    id: String,
    #[snugom(relation(target = "rev_users"))]
    sender_id: String,
    #[snugom(relation(target = "rev_users"))]
    recipient_id: String,
    #[snugom(filterable(text))]
    body: String,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [RevUser, RevPost, RevMembership, RevMessage])]
struct RevClient {
    conn: snugom::ConnectionManager,
    prefix: String,
}

// ============ Helpers ============

async fn rev_client() -> RevClient {
    let url = common::test_redis_url();
    let prefix = format!("rev_inc_{}", uuid::Uuid::new_v4());
    let mut client = RevClient::connect(&url, prefix).await.expect("connect");
    // Registers all descriptors globally (needed for find_incoming_relations)
    // and creates the child search indexes the reverse path queries.
    client.ensure_indexes().await.expect("ensure_indexes");
    client
}

async fn cleanup(client: &RevClient) {
    let pattern = format!("{}:*", client.prefix());
    let _ = snugom::cleanup_pattern(&mut client.connection(), &pattern).await;
}

async fn create_user(client: &mut RevClient, email: &str) -> String {
    client
        .rev_users()
        .create(RevUser::validation_builder().email(email.to_string()))
        .await
        .expect("create user")
        .id
}

async fn create_membership(client: &mut RevClient, user_id: &str, role: &str, joined_at: chrono::DateTime<Utc>) {
    client
        .rev_memberships()
        .create(
            RevMembership::validation_builder()
                .user_id(user_id.to_string())
                .role(role.to_string())
                .joined_at(joined_at),
        )
        .await
        .expect("create membership");
}

fn include(keys: &[(&str, RelationQueryOptions)]) -> HashMap<String, RelationQueryOptions> {
    keys.iter().map(|(k, o)| (k.to_string(), o.clone())).collect()
}

// Indexing is async on the Redis side; give FT.SEARCH a beat to catch up.
async fn settle() {
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
}

// ============ Tests ============

#[tokio::test]
async fn reverse_include_returns_children_and_isolates_by_parent() {
    let mut client = rev_client().await;
    let now = Utc::now();

    let alice = create_user(&mut client, "alice@example.com").await;
    let bob = create_user(&mut client, "bob@example.com").await;
    create_membership(&mut client, &alice, "admin", now).await;
    create_membership(&mut client, &alice, "member", now).await;
    create_membership(&mut client, &bob, "member", now).await;
    settle().await;

    let result = client
        .rev_users()
        .find_with_includes_from_params(&alice, &include(&[("rev_memberships", RelationQueryOptions::default())]))
        .await
        .expect("find with reverse include");

    // Parent deserializes; children are alice's only.
    let user: RevUser = result.deserialize_entity().expect("deserialize parent");
    assert_eq!(user.email, "alice@example.com");

    let memberships: Vec<RevMembership> = result.include("rev_memberships").expect("typed include").expect("include present");
    assert_eq!(memberships.len(), 2, "alice has exactly 2 memberships");
    assert!(memberships.iter().all(|m| m.user_id == alice), "no other user's rows leak in");
    let roles: Vec<&str> = memberships.iter().map(|m| m.role.as_str()).collect();
    assert!(roles.contains(&"admin") && roles.contains(&"member"));

    cleanup(&client).await;
}

#[tokio::test]
async fn reverse_include_empty_is_loaded_not_error() {
    let mut client = rev_client().await;
    let alice = create_user(&mut client, "lonely@example.com").await;
    settle().await;

    let result = client
        .rev_users()
        .find_with_includes_from_params(&alice, &include(&[("rev_memberships", RelationQueryOptions::default())]))
        .await
        .expect("find with reverse include");

    // Key present with empty vec ("loaded, zero rows"), total 0 — not an error, not absent.
    assert!(result.includes.contains_key("rev_memberships"));
    let memberships: Vec<RevMembership> = result.include("rev_memberships").expect("typed include").expect("include present");
    assert!(memberships.is_empty());
    assert_eq!(result.relation_metadata.get("rev_memberships").and_then(|m| m.total), Some(0));

    cleanup(&client).await;
}

#[tokio::test]
async fn reverse_include_with_limit_sort_and_metadata() {
    let mut client = rev_client().await;
    let base = Utc::now();
    let alice = create_user(&mut client, "busy@example.com").await;
    // Five memberships, increasing joined_at.
    for i in 0..5 {
        create_membership(&mut client, &alice, &format!("role{i}"), base + Duration::minutes(i)).await;
    }
    settle().await;

    let opts = RelationQueryOptions::default().with_limit(2).with_sort("-joined_at");
    let result = client
        .rev_users()
        .find_with_includes_from_params(&alice, &include(&[("rev_memberships", opts)]))
        .await
        .expect("find with reverse include");

    let memberships: Vec<RevMembership> = result.include("rev_memberships").expect("typed include").expect("include present");
    assert_eq!(memberships.len(), 2, "limit caps the page at 2");
    // Newest first: role4 then role3.
    assert_eq!(memberships[0].role, "role4");
    assert_eq!(memberships[1].role, "role3");

    let meta = result.relation_metadata.get("rev_memberships").expect("metadata present");
    assert_eq!(meta.total, Some(5), "total reflects all matching rows");
    assert_eq!(meta.has_more, Some(true), "more than the page exists");

    cleanup(&client).await;
}

#[tokio::test]
async fn reverse_include_offset_paginates() {
    let mut client = rev_client().await;
    let base = Utc::now();
    let alice = create_user(&mut client, "page@example.com").await;
    for i in 0..5 {
        create_membership(&mut client, &alice, &format!("role{i}"), base + Duration::minutes(i)).await;
    }
    settle().await;

    // Page 2 of size 2, sorted ascending: role2, role3.
    let opts = RelationQueryOptions::default().with_limit(2).with_offset(2).with_sort("joined_at");
    let result = client
        .rev_users()
        .find_with_includes_from_params(&alice, &include(&[("rev_memberships", opts)]))
        .await
        .expect("find with reverse include");

    let memberships: Vec<RevMembership> = result.include("rev_memberships").expect("typed include").expect("include present");
    let roles: Vec<&str> = memberships.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, vec!["role2", "role3"]);

    cleanup(&client).await;
}

#[tokio::test]
async fn forward_has_many_with_options_searches_child_by_fk() {
    // A forward has_many (`memberships`, declared on RevUser) requested WITH options
    // resolves via the FT.SEARCH-by-FK path (not the Lua relation-set path), shared
    // with reverse includes. The child rev_memberships declares belongs_to rev_users.
    let mut client = rev_client().await;
    let base = Utc::now();
    let alice = create_user(&mut client, "forward-opts@example.com").await;
    for i in 0..5 {
        create_membership(&mut client, &alice, &format!("role{i}"), base + Duration::minutes(i)).await;
    }
    settle().await;

    let opts = RelationQueryOptions::default().with_limit(2).with_sort("-joined_at");
    let result = client
        .rev_users()
        .find_with_includes_from_params(&alice, &include(&[("memberships", opts)]))
        .await
        .expect("find with forward has_many + options");

    // Keyed under the forward relation alias "memberships" (not the child collection).
    let memberships: Vec<RevMembership> =
        result.include("memberships").expect("typed include").expect("include present");
    assert_eq!(memberships.len(), 2, "limit caps the page at 2");
    assert_eq!(memberships[0].role, "role4");
    assert_eq!(memberships[1].role, "role3");

    let meta = result.relation_metadata.get("memberships").expect("metadata present");
    assert_eq!(meta.total, Some(5));
    assert_eq!(meta.has_more, Some(true));

    cleanup(&client).await;
}

#[tokio::test]
async fn reverse_include_ambiguous_multi_fk_errors_with_choices() {
    let mut client = rev_client().await;
    let alice = create_user(&mut client, "sender@example.com").await;
    let bob = create_user(&mut client, "recipient@example.com").await;
    client
        .rev_messages()
        .create(
            RevMessage::validation_builder()
                .sender_id(alice.clone())
                .recipient_id(bob.clone())
                .body("hello".to_string()),
        )
        .await
        .expect("create message");
    settle().await;

    // Bare collection name is ambiguous: rev_messages has sender + recipient FKs to rev_users.
    let err = client
        .rev_users()
        .find_with_includes_from_params(&alice, &include(&[("rev_messages", RelationQueryOptions::default())]))
        .await
        .expect_err("bare ambiguous include must error");
    match err {
        RepoError::InvalidRequest { message } => {
            assert!(message.contains("ambiguous"), "got: {message}");
            assert!(message.contains("rev_messages.sender"), "lists the disambiguated choices: {message}");
            assert!(message.contains("rev_messages.recipient"), "lists the disambiguated choices: {message}");
        }
        other => panic!("expected InvalidRequest, got {other:?}"),
    }

    // Disambiguated form resolves: alice is the sender of one message.
    let result = client
        .rev_users()
        .find_with_includes_from_params(&alice, &include(&[("rev_messages.sender", RelationQueryOptions::default())]))
        .await
        .expect("disambiguated include resolves");
    let sent: Vec<RevMessage> = result.include("rev_messages.sender").expect("typed include").expect("include present");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].sender_id, alice);

    // Alice is not the recipient of anything.
    let result = client
        .rev_users()
        .find_with_includes_from_params(&alice, &include(&[("rev_messages.recipient", RelationQueryOptions::default())]))
        .await
        .expect("disambiguated include resolves");
    let received: Vec<RevMessage> = result.include("rev_messages.recipient").expect("typed include").expect("include present");
    assert!(received.is_empty());

    cleanup(&client).await;
}

#[tokio::test]
async fn reverse_include_unknown_key_is_skipped_not_errored() {
    let mut client = rev_client().await;
    let alice = create_user(&mut client, "skip@example.com").await;
    settle().await;

    let result = client
        .rev_users()
        .find_with_includes_from_params(
            &alice,
            &include(&[("totally_unknown_collection", RelationQueryOptions::default())]),
        )
        .await
        .expect("unknown include is silently skipped, not an error");

    assert!(!result.includes.contains_key("totally_unknown_collection"));

    cleanup(&client).await;
}

#[tokio::test]
async fn mixed_forward_and_reverse_in_one_call() {
    let mut client = rev_client().await;
    let now = Utc::now();

    // Forward has_many: create posts, then a user linked to them via the relation set.
    let post_a = client
        .rev_posts()
        .create(RevPost::validation_builder().title("Post A".to_string()))
        .await
        .expect("create post")
        .id;
    let post_b = client
        .rev_posts()
        .create(RevPost::validation_builder().title("Post B".to_string()))
        .await
        .expect("create post")
        .id;

    let alice = client
        .rev_users()
        .create(
            RevUser::validation_builder()
                .email("mixed@example.com".to_string())
                .relation("posts", vec![post_a.clone(), post_b.clone()], Vec::new()),
        )
        .await
        .expect("create user with forward posts")
        .id;

    // Reverse: a membership belonging to the user.
    create_membership(&mut client, &alice, "admin", now).await;
    settle().await;

    let result = client
        .rev_users()
        .find_with_includes_from_params(
            &alice,
            &include(&[
                ("posts", RelationQueryOptions::default()),
                ("rev_memberships", RelationQueryOptions::default()),
            ]),
        )
        .await
        .expect("find with forward + reverse includes");

    let posts: Vec<RevPost> = result.include("posts").expect("forward include typed").expect("present");
    assert_eq!(posts.len(), 2, "forward has_many still resolves");

    let memberships: Vec<RevMembership> = result.include("rev_memberships").expect("reverse include typed").expect("present");
    assert_eq!(memberships.len(), 1, "reverse include resolves in the same call");

    cleanup(&client).await;
}

#[tokio::test]
async fn reverse_include_non_aligned_offset_returns_correct_window() {
    // offset=1,limit=2 (offset not a multiple of limit) must return rows 1..3 via a
    // native RediSearch LIMIT offset, not the floored-to-page window the old math gave.
    let mut client = rev_client().await;
    let base = Utc::now();
    let alice = create_user(&mut client, "nonaligned@example.com").await;
    for i in 0..5 {
        create_membership(&mut client, &alice, &format!("role{i}"), base + Duration::minutes(i)).await;
    }
    settle().await;

    let opts = RelationQueryOptions::default().with_limit(2).with_offset(1).with_sort("joined_at");
    let result = client
        .rev_users()
        .find_with_includes_from_params(&alice, &include(&[("rev_memberships", opts)]))
        .await
        .expect("find with reverse include");

    let memberships: Vec<RevMembership> =
        result.include("rev_memberships").expect("typed include").expect("include present");
    let roles: Vec<&str> = memberships.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, vec!["role1", "role2"], "skip(1).take(2) of ascending rows");

    let meta = result.relation_metadata.get("rev_memberships").expect("metadata present");
    assert_eq!(meta.total, Some(5));
    assert_eq!(meta.has_more, Some(true), "rows beyond offset+limit remain");

    cleanup(&client).await;
}

#[tokio::test]
async fn reverse_include_with_filter_narrows_results() {
    let mut client = rev_client().await;
    let now = Utc::now();
    let alice = create_user(&mut client, "filter@example.com").await;
    create_membership(&mut client, &alice, "admin", now).await;
    create_membership(&mut client, &alice, "member", now).await;
    create_membership(&mut client, &alice, "member", now).await;
    settle().await;

    let opts = RelationQueryOptions::default().with_filter("role:eq:admin");
    let result = client
        .rev_users()
        .find_with_includes_from_params(&alice, &include(&[("rev_memberships", opts)]))
        .await
        .expect("find with filtered reverse include");

    let memberships: Vec<RevMembership> =
        result.include("rev_memberships").expect("typed include").expect("include present");
    assert_eq!(memberships.len(), 1, "filter narrows to admin only");
    assert_eq!(memberships[0].role, "admin");

    cleanup(&client).await;
}

#[tokio::test]
async fn forward_has_many_with_options_but_no_back_fk_errors() {
    // RevUser has_many `posts` -> rev_posts, which declares NO belongs_to rev_users.
    // Requesting `posts` WITH options can't be searched, so it must fail loud, not silently drop.
    let mut client = rev_client().await;
    let alice = create_user(&mut client, "noback@example.com").await;
    settle().await;

    let opts = RelationQueryOptions::default().with_limit(2);
    let err = client
        .rev_users()
        .find_with_includes_from_params(&alice, &include(&[("posts", opts)]))
        .await
        .expect_err("options on a has_many with no back-FK must error");
    match err {
        RepoError::InvalidRequest { message } => {
            assert!(message.contains("posts"), "names the include: {message}");
            assert!(message.contains("belongs_to"), "explains the cause: {message}");
        }
        other => panic!("expected InvalidRequest, got {other:?}"),
    }

    cleanup(&client).await;
}

#[tokio::test]
async fn reverse_include_unknown_filter_or_sort_field_errors() {
    // A typo'd filter field makes RediSearch return zero rows silently; a typo'd sort
    // field makes it error (500). Both must surface as a loud InvalidRequest instead.
    let mut client = rev_client().await;
    let alice = create_user(&mut client, "fieldcheck@example.com").await;
    create_membership(&mut client, &alice, "admin", Utc::now()).await;
    settle().await;

    let opts = RelationQueryOptions::default().with_filter("nope:eq:x");
    let err = client
        .rev_users()
        .find_with_includes_from_params(&alice, &include(&[("rev_memberships", opts)]))
        .await
        .expect_err("unknown filter field must error");
    assert!(matches!(err, RepoError::InvalidRequest { .. }), "filter: got {err:?}");

    let opts = RelationQueryOptions::default().with_sort("nope");
    let err = client
        .rev_users()
        .find_with_includes_from_params(&alice, &include(&[("rev_memberships", opts)]))
        .await
        .expect_err("unknown sort field must error");
    assert!(matches!(err, RepoError::InvalidRequest { .. }), "sort: got {err:?}");

    // No false rejection: known filter + sort fields still resolve.
    let opts = RelationQueryOptions::default().with_filter("role:eq:admin").with_sort("joined_at");
    let result = client
        .rev_users()
        .find_with_includes_from_params(&alice, &include(&[("rev_memberships", opts)]))
        .await
        .expect("known filter + sort fields must work");
    let members: Vec<RevMembership> =
        result.include("rev_memberships").expect("typed include").expect("present");
    assert_eq!(members.len(), 1);

    cleanup(&client).await;
}
