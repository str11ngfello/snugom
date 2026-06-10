# Snugom API Layers

This document maps the snugom API surface from lowest to highest level of abstraction.

## Layer Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           SNUGOM API LAYERS                                  │
│                      (Bottom = Low-level, Top = High-level)                  │
└─────────────────────────────────────────────────────────────────────────────┘

  Layer 5: Macro DSL ──────────── snugom_create!, snugom_update!, etc.
      │
      ▼
  Layer 4: SnugomClient ───────── #[derive(SnugomClient)] typed clients
      │
      ▼
  Layer 3: CollectionHandle ───── Type-safe CRUD, bulk ops, queries
      │
      ▼
  Layer 2: Repository ─────────── Low-level Repo<T>, relations, indexes
      │
      ▼
  Layer 1: Builders & Queries ─── ValidationBuilder, PatchBuilder, SearchQuery
      │
      ▼
  Layer 0: Entity Definition ──── #[derive(SnugomEntity)]
```

---

## Layer 5: Macro DSL (Highest Ergonomics)

The most ergonomic way to perform mutations. Best for application code.

```rust
use snugom::{snugom_create, snugom_update, snugom_delete, snugom_upsert};

// Create
let result = snugom_create!(client, User {
    name: "Alice".to_string(),
    email: "alice@example.com".to_string(),
    created_at: Utc::now(),
}).await?;

// Update (with relation mutations)
snugom_update!(client, User(entity_id = &user_id) {
    name: "Bob".to_string(),
    posts: [
        connect post_id.clone(),
        disconnect old_post_id.clone(),
    ],
}).await?;

// Delete
snugom_delete!(client, User(&user_id)).await?;

// Upsert
snugom_upsert!(client, UserSettings(id = &key) {
    create: UserSettings { ... },
    update: UserSettings(entity_id = &key) { theme: "dark".to_string() },
}).await?;
```

**Requires:** SnugomClient (Layer 4)

**Unique capabilities:**
- Relation mutations (`connect`, `disconnect`, `delete`) in update macro

**Limitations:**
- No idempotency keys
- No version checks for optimistic locking
- No bulk operations

---

## Layer 4: SnugomClient (Typed Client)

Custom typed clients with named collection accessors.

```rust
#[derive(SnugomClient)]
#[snugom_client(entities = [User, Post, Comment])]
struct AppClient {
    conn: ConnectionManager,
    prefix: String,
}

// Usage
let client = AppClient::connect(&redis_url, "myapp").await?;

// Named accessors (generated)
let mut users = client.users();      // → CollectionHandle<User>
let mut posts = client.posts();      // → CollectionHandle<Post>

// Generic accessor (always available)
let mut users = client.collection::<User>();

// Index management
client.ensure_indexes().await?;
```

**Requires:** CollectionHandle (Layer 3)

**Unique capabilities:**
- Named accessors (`.users()`, `.posts()`) instead of `.collection::<T>()`
- Automatic index management via `ensure_indexes()`

---

## Layer 3: CollectionHandle (Type-safe CRUD)

The workhorse for CRUD operations. Returned by client accessors.

### Single Entity Operations

```rust
let mut users = client.users();

// Read
let user = users.get(&id).await?;                    // Option<User>
let user = users.get_or_error(&id).await?;           // User or NotFound
let exists = users.exists(&id).await?;               // bool
let count = users.count().await?;                    // u64

// Create
let result = users.create(builder).await?;           // CreateResult { id, responses }
let user = users.create_and_get(builder).await?;     // User

// Update
users.update(patch_builder).await?;
let user = users.update_and_get(&id, patch_builder).await?;

// Delete
users.delete(&id).await?;
users.delete_with_version(&id, expected_version).await?;

// Upsert
let result = users.upsert(create_builder, update_builder).await?;
```

### Query-based Operations

```rust
let query = SearchQuery {
    filter: vec!["status:eq:active".to_string()],
    page: Some(1),
    page_size: Some(25),
    sort_by: Some("created_at".to_string()),
    sort_order: Some("desc".to_string()),
    ..Default::default()
};

// Query
let result = users.find_many(query).await?;          // SearchResult<User>
let user = users.find_first(query).await?;           // Option<User>
let user = users.find_first_or_error(query).await?;  // User or NotFound
let count = users.count_where(query).await?;         // u64
let exists = users.exists_where(query).await?;       // bool

// Bulk mutations by query
let deleted = users.delete_many(query).await?;       // u64
let updated = users.update_many(query, |id| {
    User::patch_builder().entity_id(id).status("inactive".to_string())
}).await?;
```

### Bulk Operations by IDs

```rust
// Create many
let result = users.create_many(vec![builder1, builder2, builder3]).await?;
// → BulkCreateResult { count, ids, responses }

// Delete many
let deleted = users.delete_many_by_ids(&["id1", "id2", "id3"]).await?;

// Update many
let updated = users.update_many_by_ids(&["id1", "id2"], |id| {
    User::patch_builder().entity_id(id).status("archived".to_string())
}).await?;
```

**Requires:** Repo<T> (Layer 2), Builders (Layer 1)

**Unique capabilities:**
- Bulk operations (`create_many`, `delete_many`, `update_many`)
- Query-based mutations
- Version-checked delete (`delete_with_version`)

---

## Layer 2: Repository (Low-level Operations)

Direct access to `Repo<T>` for advanced use cases.

```rust
use snugom::Repo;

let repo: Repo<User> = Repo::new("myapp".to_string());

// Direct CRUD
repo.create_with_conn(&mut conn, builder).await?;
repo.get(&mut conn, &id).await?;
repo.update_patch_with_conn(&mut conn, patch_builder).await?;
repo.delete_with_conn(&mut conn, &id, Some(version)).await?;

// Search
repo.search_with_query(&mut conn, query).await?;

// Index management
repo.ensure_index(&mut conn).await?;

// Direct relation manipulation
repo.add_to_relation(&mut conn, &parent_id, "children", &child_id).await?;
repo.remove_from_relation(&mut conn, &parent_id, "children", &child_id).await?;
repo.get_relation_ids(&mut conn, &parent_id, "children").await?;
repo.query_related_entities::<Child>(&mut conn, &parent_id, "children", options).await?;
```

**Requires:** ConnectionManager, Builders, SearchQuery

**Unique capabilities:**
- Direct relation manipulation
- Fine-grained index control
- Access to raw Redis operations

---

## Layer 1: Builders & Queries (Data Construction)

### Mutation Builders

Generated by `#[derive(SnugomEntity)]`:

```rust
// ValidationBuilder - for creates
let builder = User::validation_builder()
    .name("Alice".to_string())
    .email("alice@example.com".to_string())
    .created_at(Utc::now())
    .idempotency_key("unique-request-id");  // Only available here!

// PatchBuilder - for updates
let patch = User::patch_builder()
    .entity_id(&user_id)
    .name("Bob".to_string())
    .version(expected_version);  // For optimistic locking
```

### Search Queries

**String-based (API-friendly):**

```rust
let query = SearchQuery {
    filter: vec![
        "status:eq:active".to_string(),
        "score:gt:100".to_string(),
    ],
    q: Some("search text".to_string()),
    page: Some(1),
    page_size: Some(25),
    sort_by: Some("created_at".to_string()),
    sort_order: Some("desc".to_string()),
};
```

**Typed FilterCondition (for complex logic):**

```rust
use snugom::search::FilterCondition;

// Simple conditions
let filter = FilterCondition::tag_eq("status", "active");
let filter = FilterCondition::numeric_gt("score", 100.0);
let filter = FilterCondition::text_contains("name", "alice");

// Boolean logic (required for OR conditions)
let visibility = FilterCondition::or([
    FilterCondition::bool_eq("public", true),
    FilterCondition::tag_eq("owner", &user_id),
]);

let complex = FilterCondition::and([
    FilterCondition::tag_eq("status", "active"),
    FilterCondition::or([
        FilterCondition::numeric_gt("priority", 5.0),
        FilterCondition::tag_eq("urgent", "true"),
    ]),
]);
```

**Unique capabilities:**
- `.idempotency_key()` - only on ValidationBuilder
- `.version()` - only on PatchBuilder
- `FilterCondition::or()` - required for OR logic (string filters are ANDed)

---

## Layer 0: Entity Definition (Foundation)

Everything starts here with `#[derive(SnugomEntity)]`:

```rust
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "myapp", collection = "users")]
struct User {
    // Identity
    #[snugom(id)]
    id: String,

    // Automatic timestamps
    #[snugom(created_at)]
    created_at: DateTime<Utc>,

    #[snugom(updated_at)]
    updated_at: DateTime<Utc>,

    // Optimistic locking
    #[snugom(version)]
    version: u64,

    // Searchable fields
    #[snugom(filterable(text))]
    name: String,

    #[snugom(filterable(tag))]
    status: String,

    #[snugom(filterable, sortable)]
    score: i64,

    // Unique constraints
    #[snugom(unique)]
    email: String,

    #[snugom(unique(case_insensitive))]
    username: String,

    // Relations
    #[serde(default)]
    #[snugom(relation(target = "posts", cascade = "delete"))]
    posts: Vec<String>,
}
```

**Generates:**
- `SnugomModel` trait impl (SERVICE, COLLECTION, get_id())
- `EntityMetadata` trait impl (entity descriptor)
- `ValidationBuilder` and `PatchBuilder`
- `SearchEntity` impl (RediSearch index schema)
- Relation metadata

---

## Capability Matrix

| Operation | Macro DSL | CollectionHandle | Builder |
|-----------|:---------:|:----------------:|:-------:|
| Create single | ✅ | ✅ | Required |
| Update single | ✅ | ✅ | Required |
| Delete single | ✅ | ✅ | N/A |
| Upsert | ✅ | ✅ | Required |
| **Bulk create** | ❌ | ✅ | Required |
| **Bulk update** | ❌ | ✅ | Required |
| **Bulk delete** | ❌ | ✅ | N/A |
| **Idempotency key** | ❌ | ❌ | ✅ |
| **Version check** | ❌ | ✅ | ✅ |
| **Relation mutations** | ✅ | ❌ | ❌ |
| **OR filters** | N/A | ❌ | ✅ FilterCondition |

---

## When to Use What

| Use Case | Recommended Layer |
|----------|-------------------|
| Simple CRUD in application code | **Macro DSL** (Layer 5) |
| Bulk operations | **CollectionHandle** (Layer 3) |
| Idempotent creates (likes, follows) | **Builder** with `.idempotency_key()` |
| Optimistic locking updates | **Builder** with `.version()` |
| OR filter conditions | **FilterCondition** (Layer 1) |
| Simple filters from HTTP params | **SearchQuery** strings (Layer 1) |
| Direct relation manipulation | **Repo** (Layer 2) |
| Reusable domain functions | **CollectionHandle** (Layer 3) |

---

## The "Happy Path" Stack

For typical application code:

```rust
// 1. Define entities (Layer 0)
#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "myapp", collection = "users")]
struct User { ... }

// 2. Define typed client (Layer 4)
#[derive(SnugomClient)]
#[snugom_client(entities = [User, Post])]
struct AppClient { conn: ConnectionManager, prefix: String }

// 3. Use macro DSL for mutations (Layer 5)
let result = snugom_create!(client, User {
    name: "Alice".into(),
    ...
}).await?;

snugom_update!(client, User(entity_id = &result.id) {
    name: "Bob".into()
}).await?;

// 4. Use CollectionHandle for queries (Layer 3)
let active_users = client.users().find_many(SearchQuery {
    filter: vec!["status:eq:active".to_string()],
    ..Default::default()
}).await?;
```

---

## See Also

- `src/examples/client/` - High-level client examples (23 examples)
- `src/examples/repo/` - Low-level repo examples (13 examples)
- `tests/client_integration.rs` - Comprehensive API tests
