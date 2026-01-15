# SnugOM

SnugOM is a Redis-based ORM with a focus on developer experience. It leverages the advanced search features of Redis 8, adds relational data modeling, atomic mutations, lock-free optimistic concurrency control, idempotency keys and more. All functionality is reachable through a set of macros that are compatible with serde and garde. 

Define entities with `#[derive(SnugomEntity)]`, and use the Prisma-style `SnugomClient` for CRUD operations, or the macro DSL (`snugom_create!`, `snugom_update!`, `snugom_delete!`, `snugom_upsert!`) for complex nested operations with relations, validation, and search. 

## Table of Contents
- [SnugOM](#snugom)
  - [Table of Contents](#table-of-contents)
  - [Quick Example](#quick-example)
  - [Core Concepts](#core-concepts)
  - [Entity Definition](#entity-definition)
    - [Container Attributes](#container-attributes)
    - [Field Attributes](#field-attributes)
  - [CRUD Operations](#crud-operations)
    - [Simple CRUD with SnugomClient](#simple-crud-with-snugomclient)
    - [Complex Nested Operations](#complex-nested-operations)
    - [Relation Mutations](#relation-mutations)
  - [Search, Filter, and Sort](#search-filter-and-sort)
    - [Filter Operators](#filter-operators)
    - [Programmatic Filters](#programmatic-filters)
  - ["I Want To..." Reference](#i-want-to-reference)
    - [Numeric Fields (u32, i64, f64)](#numeric-fields-u32-i64-f64)
    - [Boolean Fields](#boolean-fields)
    - [Enum Fields](#enum-fields)
    - [String Fields - Full-Text Search (TEXT)](#string-fields---full-text-search-text)
    - [String Fields - Exact Match (TAG)](#string-fields---exact-match-tag)
    - [DateTime Fields](#datetime-fields)
    - [Array Fields (Vec)](#array-fields-vec)
    - [Type Inference Rules](#type-inference-rules)
  - [Validation Rules](#validation-rules)
  - [Relations and Cascades](#relations-and-cascades)
    - [Defining Relations](#defining-relations)
    - [Cascade Policies](#cascade-policies)
  - [Advanced Topics](#advanced-topics)
    - [Idempotency](#idempotency)
    - [Optimistic Concurrency](#optimistic-concurrency)
    - [Lower-Level `snug!` Macro](#lower-level-snug-macro)
    - [Direct Repo API](#direct-repo-api)
  - [Redis Setup](#redis-setup)
  - [Schema Migrations \& CLI](#schema-migrations--cli)
  - [Development](#development)
  - [Examples](#examples)
    - [Running Examples](#running-examples)
  - [Future Improvements](#future-improvements)
  - [Contributing](#contributing)

## Quick Example

```rust
use chrono::{DateTime, Utc};
use snugom::{SnugomClient, SnugomEntity};

// 1. Define your entity with service and collection
#[derive(SnugomEntity, serde::Serialize, serde::Deserialize, Default)]
#[snugom(schema = 1, service = "guild", collection = "guilds", default_sort = "-created_at")]
pub struct Guild {
    #[snugom(id)]
    pub guild_id: String,

    #[snugom(searchable, sortable, validate(length(min = 1, max = 200)))]
    pub name: String,

    #[snugom(filterable)]
    pub visibility: GuildVisibility,

    #[snugom(filterable, sortable)]
    pub member_count: u32,

    #[snugom(created_at)]
    pub created_at: DateTime<Utc>,
}

// 2. Define your client - entities are auto-discovered
#[derive(SnugomClient)]
#[snugom(prefix = "myapp")]
pub struct MyClient;

// 3. Use client for simple CRUD, macros for complex nested operations
async fn example() -> anyhow::Result<()> {
    let client = MyClient::connect("redis://localhost").await?;
    client.ensure_indexes().await?;

    // Create - returns the entity
    let guild = client.guilds().create(Guild {
        name: "Dragon Knights".into(),
        visibility: GuildVisibility::Public,
        member_count: 1,
        ..Default::default()
    }).await?;

    // Read
    let guild = client.guilds().get(&guild.guild_id).await?; // Option<Guild>
    let guild = client.guilds().get_or_error(&guild.guild_id).await?; // Guild (errors if not found)

    // Update - returns updated entity
    let guild = client.guilds().update(&guild.guild_id, Guild::patch()
        .name("Dragon Knights Elite".into())
        .member_count(5)
    ).await?;

    // Delete
    client.guilds().delete(&guild.guild_id).await?;

    Ok(())
}
```

## Core Concepts

| Concept | Description |
|---------|-------------|
| `SnugomEntity` | Derive macro that generates metadata, builders, and search implementations |
| `SnugomClient` | Derive macro that generates a typed client with collection accessors |
| `CollectionHandle<T>` | Type-safe accessor for CRUD operations (via `client.guilds()`) |
| `snugom_create!` | Macro for creating entities with nested relations |
| `snugom_update!` | Macro for updating entities with relation mutations |
| `snugom_delete!` | Macro for deleting entities with cascade control |
| `snugom_upsert!` | Macro for upsert operations (update or create) |
| `snug!` | Lower-level macro for building payloads (used with `repo.create()`) |
| `SearchEntity` | Auto-derived trait enabling search/filter/sort capabilities |

## Entity Definition

### Container Attributes

```rust
#[derive(SnugomEntity)]
#[snugom(schema = 1, service = "myapp", collection = "entities", default_sort = "-created_at")]
pub struct MyEntity { ... }

// Compound unique: (tenant_id, name) must be unique together
#[derive(SnugomEntity)]
#[snugom(schema = 1, service = "myapp", collection = "scoped", unique_together = ["tenant_id", "name"])]
pub struct TenantScopedEntity { ... }
```

| Attribute | Required | Description |
|-----------|----------|-------------|
| `schema = N` | Yes | Schema version stored in metadata |
| `service = "name"` | Yes | Service name for key prefixing |
| `collection = "name"` | Yes | Collection name for key prefixing |
| `default_sort = "field"` | No | Default sort field. Prefix with `-` for descending |
| `unique_together = ["f1", "f2"]` | No | Compound unique constraint across multiple fields |

### Field Attributes

```rust
#[snugom(id)]
pub entity_id: String,

#[snugom(searchable, sortable, validate(length(min = 1, max = 200)))]
pub name: String,

#[snugom(filterable)]
pub status: MyEnum,

#[snugom(filterable, sortable)]
pub count: u32,

#[snugom(created_at)]
pub created_at: DateTime<Utc>,

#[snugom(relation(target = "members", cascade = "delete"))]
pub members: Vec<String>,

#[snugom(unique, filterable(tag))]
pub slug: String,  // Enforces uniqueness across all entities
```

| Attribute | Description |
|-----------|-------------|
| `id` | Primary identifier field (auto-generated if not provided) |
| `filterable` | Expose to API for filtering via `?filter=field:op:value` |
| `filterable(tag)` | Force TAG type (exact match) for strings |
| `filterable(text)` | Force TEXT type (full-text) for strings |
| `sortable` | Enable sorting via `?sort_by=field` |
| `searchable` | Include in full-text `?q=` search queries |
| `datetime` | Create numeric mirror field (`field_ts`) for sorting |
| `created_at` | Auto-set to `Utc::now()` on create |
| `updated_at` | Auto-set to `Utc::now()` on create and update |
| `validate(...)` | Apply validation rules (see [Validation Rules](#validation-rules)) |
| `relation(target = "...", cascade = "...")` | Define relationship |
| `unique` | Enforce SQL-like UNIQUE constraint within collection |
| `unique(case_insensitive)` | Case-insensitive unique ("Foo" == "foo") |

## CRUD Operations

### Simple CRUD with SnugomClient

For simple operations, use the `SnugomClient` directly without macros:

```rust
#[derive(SnugomClient)]
#[snugom(prefix = "myapp")]
pub struct MyClient;

let client = MyClient::connect("redis://localhost").await?;
client.ensure_indexes().await?;

// ============ Single Record by ID ============
let guild = client.guilds().get(&id).await?;              // Option<T>
let guild = client.guilds().get_or_error(&id).await?;     // T (errors if not found)
let exists = client.guilds().exists(&id).await?;          // bool

// ============ Create ============
let guild = client.guilds().create(Guild {
    name: "Dragon Knights".into(),
    visibility: GuildVisibility::Public,
    member_count: 1,
    ..Default::default()
}).await?;  // Returns T

// ============ Update ============
let guild = client.guilds().update(&id, Guild::patch()
    .name("New Name".into())
    .member_count(10)
).await?;  // Returns T

// ============ Delete ============
client.guilds().delete(&id).await?;  // Returns ()

// ============ Query-based Reads ============
let guild = client.guilds().find_first(query).await?;             // Option<T>
let guild = client.guilds().find_first_or_error(query).await?;    // T (errors if not found)
let guilds = client.guilds().find_many(query).await?;             // SearchResult<T>
let total = client.guilds().count().await?;                       // u64
let total = client.guilds().count_where(query).await?;            // u64
let exists = client.guilds().exists_where(query).await?;          // bool

// ============ Bulk Operations ============
let result = client.guilds().create_many(entities).await?;        // BulkCreateResult
let count = client.guilds().update_many(query, patch).await?;     // u64
let count = client.guilds().delete_many(query).await?;            // u64
```

### Complex Nested Operations

For nested creates and relation mutations, use the macro DSL:

```rust
use snugom::{snugom_create, snugom_update, snugom_delete, snugom_upsert};

// Nested create - create parent with children in one operation
let guild = snugom_create!(&client.guilds(), &mut conn, Guild {
    name: "Dragon Knights".to_string(),
    visibility: GuildVisibility::Public,
    member_count: 1u32,
    guild_members: [
        create GuildMember {
            user_id: "user123".to_string(),
            role: MemberRole::Leader,
        }
    ],
}).await?;

// Update with nested create
snugom_update!(&client.guilds(), &mut conn, Guild(entity_id = guild_id) {
    name: "New Name".to_string(),
    member_count: 10u32,
    guild_members: [
        create GuildMember {
            user_id: "user456".to_string(),
            role: MemberRole::Member,
        }
    ],
}).await?;

// Delete
snugom_delete!(&client.guilds(), &mut conn, Guild(entity_id = guild_id)).await?;

// Upsert - update if exists, create if not
let result = snugom_upsert!(&client.guilds(), &mut conn, Guild() {
    update: Guild(entity_id = guild_id) {
        member_count: 5u32,
    },
    create: Guild {
        name: "New Guild".to_string(),
        member_count: 1u32,
    }
}).await?;
```

### Relation Mutations

```rust
snugom_update!(&client.guilds(), &mut conn, Guild(entity_id = guild_id) {
    guild_members: [
        connect member_id,           // Attach existing entity
        disconnect old_member_id,    // Remove relationship (keep entity)
        delete stale_member_id,      // Delete entity (if cascade allows)
        create GuildMember { ... },  // Create and attach
    ],
}).await?;
```

## Search, Filter, and Sort

Entities with `filterable`, `sortable`, or `searchable` attributes auto-implement `SearchEntity`:

```rust
use snugom::search::{SearchQuery, SearchEntity};

// Build search parameters from HTTP query
let query = SearchQuery {
    page: Some(1),
    page_size: Some(25),
    sort_by: Some("created_at".to_string()),
    sort_order: Some(SortOrder::Desc),
    q: Some("dragon".to_string()),  // Full-text search
    filter: vec![
        "visibility:eq:public".to_string(),
        "member_count:range:10,".to_string(),
    ],
};

// Execute search
let params = query.with_text_query(
    Guild::allowed_sorts(),
    Guild::default_sort(),
    Guild::map_filter,
    Guild::text_search_fields(),
)?;

let results = repo.search(&mut conn, params).await?;
// results.items: Vec<Guild>
// results.total: u64
// results.has_more(): bool
```

### Filter Operators

| Operator | Syntax | Description | Example |
|----------|--------|-------------|---------|
| `eq` | `field:eq:value` | Exact match (TAG) or prefix (TEXT) | `status:eq:active` |
| `range` | `field:range:min,max` | Numeric range (inclusive) | `count:range:10,50` |
| `bool` | `field:bool:value` | Boolean match | `active:bool:true` |
| `prefix` | `field:prefix:value` | Text prefix match | `path:prefix:config/` |
| `contains` | `field:contains:value` | Text contains | `desc:contains:error` |
| `exact` | `field:exact:value` | Exact phrase match | `name:exact:John Doe` |
| `fuzzy` | `field:fuzzy:value` | Fuzzy/typo-tolerant match | `name:fuzzy:jonh` |

### Programmatic Filters

Use `FilterCondition` for complex queries:

```rust
use snugom::search::{SearchParams, FilterCondition};

let params = SearchParams::new()
    .with_condition(FilterCondition::or([
        FilterCondition::bool_eq("private", false),
        FilterCondition::tag_eq("owner", "user123"),
    ]))
    .with_condition(FilterCondition::tag_eq("status", "active"))
    .with_page(1, 25);
```

## "I Want To..." Reference

This table maps your intent to the correct field attributes.

### Numeric Fields (u32, i64, f64)

| I want to... | Attributes | API Example |
|--------------|------------|-------------|
| Filter by exact number | `#[snugom(filterable)]` | `?filter=count:eq:50` |
| Filter by number range | `#[snugom(filterable)]` | `?filter=count:range:10,100` |
| Sort by number | `#[snugom(sortable)]` | `?sort_by=count` |
| Filter AND sort | `#[snugom(filterable, sortable)]` | `?filter=count:range:10,&sort_by=count` |
| Sort only (no client filter) | `#[snugom(sortable)]` | `?sort_by=xp` (but NOT `?filter=xp:...`) |

### Boolean Fields

| I want to... | Attributes | API Example |
|--------------|------------|-------------|
| Filter by boolean | `#[snugom(filterable)]` | `?filter=active:bool:true` |
| Filter optional boolean | `#[snugom(filterable)]` on `Option<bool>` | `?filter=verified:bool:true` |

### Enum Fields

| I want to... | Attributes | API Example |
|--------------|------------|-------------|
| Filter by enum value | `#[snugom(filterable)]` | `?filter=status:eq:active` |
| Filter multiple values | `#[snugom(filterable)]` | `?filter=status:eq:active\|pending` |
| Sort by enum (alphabetic) | `#[snugom(filterable, sortable)]` | `?sort_by=status` |

### String Fields - Full-Text Search (TEXT)

| I want to... | Attributes | API Example |
|--------------|------------|-------------|
| Full-text search only | `#[snugom(searchable)]` | `?q=dragon` finds "Dragon Knights" |
| Full-text + sortable | `#[snugom(searchable, sortable)]` | `?q=dragon&sort_by=name` |
| Full-text + prefix filter | `#[snugom(searchable, filterable(text))]` | `?q=dragon` AND `?filter=name:prefix:dragon` |

### String Fields - Exact Match (TAG)

| I want to... | Attributes | API Example |
|--------------|------------|-------------|
| Filter exact string | `#[snugom(filterable(tag))]` | `?filter=slug:eq:dragon-knights` |
| Filter multiple strings | `#[snugom(filterable(tag))]` | `?filter=region:eq:us-west\|us-east` |
| Filter + sort exact string | `#[snugom(filterable(tag), sortable)]` | `?filter=region:eq:us&sort_by=region` |

### DateTime Fields

| I want to... | Attributes | API Example |
|--------------|------------|-------------|
| Filter by date range | `#[snugom(datetime, filterable)]` | `?filter=created_at:range:1704067200000,` |
| Sort by date | `#[snugom(datetime, sortable)]` | `?sort_by=created_at` |
| Auto-set on create | `#[snugom(created_at)]` | (auto-populated, sortable, filterable) |
| Auto-set on update | `#[snugom(updated_at)]` | (auto-populated, sortable, filterable) |

### Array Fields (Vec<String>)

| I want to... | Attributes | API Example |
|--------------|------------|-------------|
| Filter by tag in array | `#[snugom(filterable)]` | `?filter=tags:eq:gaming` (any match) |
| Filter multiple tags | `#[snugom(filterable)]` | `?filter=tags:eq:gaming\|competitive` |

### Type Inference Rules

| Rust Type | Inferred Index Type | Notes |
|-----------|---------------------|-------|
| `u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32`, `i64` | `NUMERIC` | Integers |
| `f32`, `f64` | `NUMERIC` | Floating point |
| `bool` | `TAG` | Stored as "true"/"false" |
| `enum` | `TAG` | Enum variant names |
| `String` | **Must specify** | Use `searchable` (TEXT) or `filterable(tag)` (TAG) |
| `Vec<String>` | `TAG` | Array of tags |
| `DateTime<Utc>` | `NUMERIC` | Via epoch millisecond mirror |

## Validation Rules

Apply validation via `#[snugom(validate(...))]`:

| Rule | Description | Example |
|------|-------------|---------|
| `length(min = ?, max = ?)` | String/array length bounds | `validate(length(min = 1, max = 200))` |
| `range(min = ?, max = ?)` | Numeric range | `validate(range(min = 0, max = 100))` |
| `regex("pattern")` | Regex match | `validate(regex("^[a-z]+$"))` |
| `enum("A", "B", case_insensitive)` | Allowed values | `validate(enum("active", "inactive"))` |
| `email` | Valid email format | `validate(email)` |
| `url` | Valid URL format | `validate(url)` |
| `uuid` | Valid UUID format | `validate(uuid)` |
| `required_if(expr)` | Required when condition true | `validate(required_if(self.status == "active"))` |
| `forbidden_if(expr)` | Forbidden when condition true | `validate(forbidden_if(self.deleted))` |
| `unique` | Unique within collection | `validate(unique)` |
| `each(...)` | Apply to Vec elements | `validate(each(length(max = 50)))` |
| `custom(path = fn)` | Custom validator function | `validate(custom(path = my::validator))` |

## Relations and Cascades

### Defining Relations

```rust
#[derive(SnugomEntity)]
pub struct Guild {
    #[snugom(id)]
    pub guild_id: String,

    // has_many with cascade delete
    #[snugom(relation(target = "guild_members", cascade = "delete"))]
    pub guild_members: Vec<String>,

    // belongs_to (foreign key)
    #[snugom(relation)]
    pub owner_id: String,
}

#[derive(SnugomEntity)]
pub struct GuildMember {
    #[snugom(id)]
    pub member_id: String,

    // Reference back to parent
    #[snugom(relation, filterable(tag))]
    pub guild_id: String,
}
```

### Cascade Policies

| Policy | Behavior |
|--------|----------|
| `cascade = "delete"` | Recursively delete related entities |
| `cascade = "detach"` | Remove relationship but keep entities |
| `cascade = "none"` | No automatic handling |

## Advanced Topics

### Idempotency

Prevent duplicate operations with idempotency keys:

```rust
// Using the validation builder with idempotency key
let guild = repo.create_with_conn(
    &mut conn,
    Guild::validation_builder()
        .name("Dragon Knights".to_string())
        .idempotency_key("create-guild-abc123"),
).await?;
// Repeat calls with same key return cached result
```

### Optimistic Concurrency

Guard against stale updates:

```rust
snugom_update!(&repo, &mut conn, Guild(entity_id = id, expected_version = 5) {
    name: "Updated Name".to_string(),
}).await?;
// Raises RepoError::VersionConflict if version != 5
```

### Lower-Level `snug!` Macro

For building payloads without executing:

```rust
// Build payload
let payload = snugom::snug! {
    Guild {
        name: "Dragon Knights",
        visibility: GuildVisibility::Public,
    }
};

// Execute separately
let created = repo.create(&mut executor, payload).await?;
```

### Direct Repo API

```rust
let repo: Repo<Guild> = Repo::new("my_prefix");

// CRUD
let entity = repo.get(&mut conn, "entity_id").await?;
let exists = repo.exists(&mut conn, "entity_id").await?;
let count = repo.count(&mut conn).await?;

// Search
repo.ensure_search_index(&mut conn).await?;
let results = repo.search(&mut conn, params).await?;
```

## Redis Setup

SnugOM requires Redis with RediSearch and RedisJSON modules:

```bash
docker run --rm -p 6379:6379 redis/redis-stack-server:latest
```

## Schema Migrations & CLI

SnugOM includes a powerful migration system with automatic schema change detection. The `snugom` CLI scans your entity definitions, generates migration files, and manages deployment to Redis.

### Key Features

- **Automatic Change Detection** — No manual version bumping; the CLI scans your entity structs and detects additions, removals, and modifications
- **Smart Migration Classification** — Changes are classified as `BASELINE` (new entity), `AUTO` (safe transformations), `STUB` (requires custom code), or `METADATA_ONLY`
- **Dry-Run Mode** — Preview migrations before applying them to production
- **Schema Snapshots** — Point-in-time JSON snapshots enable accurate diffing
- **Uniqueness Validation** — Check for duplicate values before adding unique constraints

### Quick Start

```bash
# Initialize SnugOM in your project
snugom init

# Create a migration after changing entities
snugom migrate create --name add_user_avatar

# Preview what would be migrated
snugom migrate deploy --dry-run

# Apply pending migrations
snugom migrate deploy

# Check schema version distribution in Redis
snugom schema status

# Preview pending changes without creating a migration
snugom schema diff
```

### CLI Commands Overview

| Command | Description |
|---------|-------------|
| `snugom init` | Initialize SnugOM project structure |
| `snugom migrate create --name <name>` | Generate migration from schema changes |
| `snugom migrate deploy` | Apply pending migrations to Redis |
| `snugom migrate resolve <name>` | Manually mark migration status |
| `snugom schema status` | Show schema version distribution |
| `snugom schema diff` | Preview pending schema changes |
| `snugom schema validate` | Check field uniqueness before constraints |

For comprehensive documentation including workflows, examples, and all CLI options, see the [CLI Guide](src/bin/snugom/CLI_GUIDE.md).

## Development

```bash
# Run tests
cargo test -p snugom

# Run specific test suite
cargo test -p snugom --test social_network

# Format and lint
cargo fmt && cargo clippy -- -D warnings
```

## Examples

SnugOM includes runnable examples in `src/examples/`. Each example is self-contained and demonstrates specific features:

| Example | Description |
|---------|-------------|
| `example01_hello_entity` | Basic CRUD with builders and repos |
| `example02_belongs_to` | One-to-one (`belongs_to`) relationships with cascade delete |
| `example03_has_many` | Parent-child (`has_many`) relationships with cascade delete |
| `example04_many_to_many` | Many-to-many connect/disconnect via `snug!` patch directives |
| `example05_timestamps` | Auto-managed `created_at`/`updated_at` and epoch mirrors |
| `example06_validation_rules` | Full validation DSL: length, range, regex, email, url, uuid, required_if, forbidden_if, custom validators |
| `example07_patch_updates` | Partial updates with validation and immutable field handling |
| `example08_search_filters` | RediSearch queries with TAG/NUMERIC filters and sorting |
| `example09_cascade_strategies` | Cascade delete vs detach strategies with depth guards |
| `example10_idempotency_versions` | Idempotency keys and optimistic concurrency version checks |
| `example13_relation_mutations` | Mixed relation mutations (connect, disconnect, delete) with `snugom_update!` |
| `example14_search_manager` | `SearchQuery` helpers with filter parsing |
| `example15_unique_constraints` | SQL-like UNIQUE constraints: single-field, case-insensitive, compound |
| `example_prisma_client` | Prisma-style `SnugomClient` usage patterns |
| `example99_social_network` | Full tour: nested creates, cascades, idempotency, relations, upsert using macro DSL |

### Running Examples

```bash
# Run all examples as tests
cargo test -p snugom --lib examples

# Run a specific example
cargo test -p snugom --lib example01_hello_entity
```

## Contributing

We welcome contributions! Please feel free to submit a pull request.
