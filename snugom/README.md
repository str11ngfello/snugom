# SnugOM

SnugOM is a Redis-based ORM that takes inspiration from Prisma's wonderful DX. It leverages the advanced search features of Redis 8, adds relational data modeling, atomic mutations, lock free optimistic concurrency control, idempotency keys and more. All functionality is reachable through a set of macros that are compatible with serde and garde. 

Define entities with `#[derive(SnugomEntity)]`, and use the `run!` macro for declarative CRUD operations with nested relations, validation, and search. 

## Table of Contents
- [SnugOM](#snugom)
  - [Table of Contents](#table-of-contents)
  - [Quick Example](#quick-example)
  - [Core Concepts](#core-concepts)
  - [Entity Definition](#entity-definition)
    - [Container Attributes](#container-attributes)
    - [Field Attributes](#field-attributes)
  - [Bundle Registration](#bundle-registration)
  - [CRUD Operations with `run!`](#crud-operations-with-run)
    - [Create](#create)
    - [Get](#get)
    - [Update](#update)
    - [Delete](#delete)
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
  - [Development](#development)
  - [Examples](#examples)
    - [Running Examples](#running-examples)
  - [Future Improvements](#future-improvements)
  - [Contributing](#contributing)
  - [License](#license)
  - [License](#license-1)

## Quick Example

```rust
use chrono::{DateTime, Utc};
use snugom::{SnugomEntity, bundle, repository::Repo};

// 1. Define your entity
#[derive(SnugomEntity, serde::Serialize, serde::Deserialize)]
#[snugom(version = 1, default_sort = "-created_at")]
pub struct Guild {
    #[snugom(id)]
    pub guild_id: String,

    #[snugom(searchable, sortable, validate(length(min = 1, max = 200)))]
    pub name: String,

    #[snugom(filterable)]
    pub visibility: GuildVisibility,

    #[snugom(filterable, sortable)]
    pub member_count: u32,

    #[snugom(datetime(epoch_millis), created_at, filterable, sortable)]
    pub created_at: DateTime<Utc>,
}

// 2. Register in a bundle
bundle! {
    service: "guild",
    entities: {
        Guild => "guilds",
    }
}

// 3. Use run! for CRUD operations
async fn example(repo: &Repo<Guild>, conn: &mut ConnectionManager) {
    // Create
    let created = snugom::run! {
        repo, conn,
        create => Guild {
            name: "Dragon Knights",
            visibility: GuildVisibility::Public,
            member_count: 1u32,
        }
    }?;

    // Get
    let guild = snugom::run! {
        repo, conn,
        get => Guild(entity_id = &created.id)
    }?;

    // Update
    snugom::run! {
        repo, conn,
        update => Guild(entity_id = &created.id) {
            name: "Dragon Knights Elite",
            member_count: 5u32,
        }
    }?;

    // Delete
    snugom::run! {
        repo, conn,
        delete => Guild(entity_id = &created.id)
    }?;
}
```

## Core Concepts

| Concept | Description |
|---------|-------------|
| `SnugomEntity` | Derive macro that generates metadata, builders, and search implementations |
| `bundle!` | Registers entities with service/collection names, validates relations |
| `Repo<T>` | Repository providing CRUD and search operations |
| `run!` | Declarative macro for create/get/update/delete operations |
| `snug!` | Lower-level macro for building payloads (used with `repo.create()`) |
| `SearchEntity` | Auto-derived trait enabling search/filter/sort capabilities |

## Entity Definition

### Container Attributes

```rust
#[derive(SnugomEntity)]
#[snugom(version = 1, default_sort = "-created_at")]
pub struct MyEntity { ... }
```

| Attribute | Required | Description |
|-----------|----------|-------------|
| `version = N` | Yes | Schema version stored in metadata |
| `default_sort = "field"` | No | Default sort field. Prefix with `-` for descending |

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

#[snugom(datetime(epoch_millis), created_at, filterable, sortable)]
pub created_at: DateTime<Utc>,

#[snugom(relation(target = "members", cascade = "delete"))]
pub members: Vec<String>,
```

| Attribute | Description |
|-----------|-------------|
| `id` | Primary identifier field (auto-generated if not provided) |
| `filterable` | Expose to API for filtering via `?filter=field:op:value` |
| `filterable(tag)` | Force TAG type (exact match) for strings |
| `filterable(text)` | Force TEXT type (full-text) for strings |
| `sortable` | Enable sorting via `?sort_by=field` |
| `searchable` | Include in full-text `?q=` search queries |
| `datetime(epoch_millis)` | Create numeric mirror field (`field_ts`) for sorting |
| `created_at` | Auto-set to `Utc::now()` on create |
| `updated_at` | Auto-set to `Utc::now()` on create and update |
| `validate(...)` | Apply validation rules (see [Validation Rules](#validation-rules)) |
| `relation(target = "...", cascade = "...")` | Define relationship |

## Bundle Registration

The `bundle!` macro registers entities with their service and collection names:

```rust
bundle! {
    service: "guild",
    entities: {
        Guild => "guilds",
        GuildMember => "guild_members",
        GuildApplication => "guild_applications",
    }
}
```

This generates:
- `guild::ensure_indexes(conn, prefix)` - Creates RediSearch indexes for all entities
- Key prefixes: `{prefix}:guild:guilds:{id}`, `{prefix}:guild:guild_members:{id}`, etc.
- Compile-time validation that relation targets exist in the bundle

## CRUD Operations with `run!`

The `run!` macro is the primary way to perform database operations:

### Create

```rust
let created = snugom::run! {
    &repo, &mut conn,
    create => Guild {
        name: "Dragon Knights",
        visibility: GuildVisibility::Public,
        member_count: 1u32,
        // Nested creation
        guild_members: [
            create GuildMember {
                user_id: "user123",
                role: MemberRole::Leader,
            }
        ],
    }
}?;
```

### Get

```rust
let guild = snugom::run! {
    &repo, &mut conn,
    get => Guild(entity_id = &guild_id)
}?;
// Returns Option<Guild>
```

### Update

```rust
snugom::run! {
    &repo, &mut conn,
    update => Guild(entity_id = &guild_id) {
        name: "New Name",
        member_count: 10u32,
        // Optional fields: use `?` suffix
        description?: Some("Updated description"),
    }
}?;
```

### Delete

```rust
snugom::run! {
    &repo, &mut conn,
    delete => Guild(entity_id = &guild_id)
}?;
```

### Relation Mutations

```rust
snugom::run! {
    &repo, &mut conn,
    update => Guild(entity_id = &guild_id) {
        guild_members: [
            connect member_id,           // Attach existing entity
            disconnect old_member_id,    // Remove relationship (keep entity)
            delete stale_member_id,      // Delete entity (if cascade allows)
            create GuildMember { ... },  // Create and attach
        ],
    }
}?;
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
| Filter by date range | `#[snugom(datetime(epoch_millis), filterable)]` | `?filter=created_at:range:1704067200000,` |
| Sort by date | `#[snugom(datetime(epoch_millis), sortable)]` | `?sort_by=created_at` |
| Auto-set on create | `#[snugom(datetime(epoch_millis), created_at)]` | (auto-populated) |
| Auto-set on update | `#[snugom(datetime(epoch_millis), updated_at)]` | (auto-populated) |

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
snugom::run! {
    &repo, &mut conn,
    create => Guild(idempotency_key = "create-guild-abc123") {
        name: "Dragon Knights",
    }
}?;
// Repeat calls with same key return cached result
```

### Optimistic Concurrency

Guard against stale updates:

```rust
snugom::run! {
    &repo, &mut conn,
    update => Guild(entity_id = &id, expected_version = 5) {
        name: "Updated Name",
    }
}?;
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
| `example01_hello_entity` | Basic CRUD with builders and repos (no `run!` macro) |
| `example02_belongs_to` | One-to-one (`belongs_to`) relationships with cascade delete |
| `example03_has_many` | Parent-child (`has_many`) relationships with cascade delete |
| `example04_many_to_many` | Many-to-many connect/disconnect via `snug!` patch directives |
| `example05_timestamps` | Auto-managed `created_at`/`updated_at` and epoch mirrors |
| `example06_validation_rules` | Full validation DSL: length, range, regex, email, url, uuid, required_if, forbidden_if, custom validators |
| `example07_patch_updates` | Partial updates with validation and immutable field handling |
| `example08_search_filters` | RediSearch queries with TAG/NUMERIC filters and sorting |
| `example09_cascade_strategies` | Cascade delete vs detach strategies with depth guards |
| `example10_idempotency_versions` | Idempotency keys and optimistic concurrency version checks |
| `example11_run_macro_crud` | Basic CRUD using the `run!` macro |
| `example12_run_macro_nested` | Nested create/update/upsert flows with `run!` macro |
| `example13_relation_mutations` | Mixed relation mutations (connect, disconnect, delete) |
| `example14_search_manager` | `SearchQuery` helpers with filter parsing |
| `example15_unique_constraints` | SQL-like UNIQUE constraints: single-field, case-insensitive, compound |
| `example99_social_network` | Full tour: nested creates, cascades, idempotency, relations, upsert |

### Running Examples

```bash
# Run all examples as tests
cargo test -p snugom --lib examples

# Run a specific example
cargo test -p snugom --lib example01_hello_entity
```

## Future Improvements

[ ] The redis connection is injected into macros as a dependency. It's clean but noise I'd ultimately like to remove. 

## Contributing

We welcome contributions! Please feel free to submit a pull request.

## License

MIT

## License

MIT