# SnugOM Prisma-Style Ergonomics Implementation Plan

## Overview

Transform SnugOM from a repo+connection pass-everywhere pattern to a client-centric API with auto-discovery. Simple CRUD uses direct methods; complex nested operations use the macro DSL.

**Legacy (Verbose) - DEPRECATED:**

```rust
// The run! macro has been removed. This was the old pattern:
let mut conn = self.conn.clone();
let created = snugom::run! {  // NO LONGER EXISTS
    &self.repo, &mut conn,
    create => Guild { ... }
}.await?;
```

**Current (Ergonomic):**

```rust
// Simple CRUD - direct methods with structs
let guild = snugom.guilds().create(Guild { name: "Knights".into(), ..Default::default() }).await?;
let guild = snugom.guilds().get(&id).await?;
snugom.guilds().delete(&id).await?;

// Complex nested ops - macro DSL
let guild = snugom.create! {
    Guild {
        name: "Knights",
        members: [
            create GuildMember { user_id: "u1", role: Role::Leader },
        ],
    }
}.await?;
```

## Architecture Summary

### Core Components

1. **SnugomClient** - Central client owning a `ConnectionManager` clone
2. **CollectionHandle<T>** - Type-safe accessor for simple CRUD operations
3. **Macro DSL** - `snugom.create!`, `snugom.update!`, `snugom.upsert!`, `snugom.delete!` for complex ops
4. **Auto-Registration** - Compile-time entity discovery via `inventory` crate from entity derive

### Key Design Decisions

| Decision            | Choice                                | Rationale                                              |
| ------------------- | ------------------------------------- | ------------------------------------------------------ |
| Entity discovery    | `inventory` from entity derive        | Zero-config, no separate `bundle!` needed              |
| Connection handling | Clone `ConnectionManager` into client | Cheap clone, internal multiplexing                     |
| Simple CRUD         | Direct methods with structs           | Clean, no builder overhead                             |
| Complex nested ops  | Macro DSL                             | More expressive than fluent builders, less boilerplate |
| Migration strategy  | Vertical slice (Guild first)          | Validate pattern before mass migration                 |

---

## API Design

### Complete CollectionHandle API

```rust
let snugom = Snugom::connect("redis://localhost").await?;

// ============ Single Record by ID ============
let guild = snugom.guilds().get(&id).await?;                      // Option<T>
let guild = snugom.guilds().get_or_error(&id).await?;             // T (errors if not found)
let exists = snugom.guilds().exists(&id).await?;                  // bool

// ============ Query-based Reads ============
let guild = snugom.guilds().find_first(query).await?;             // Option<T>
let guild = snugom.guilds().find_first_or_error(query).await?;    // T (errors if not found)
let guilds = snugom.guilds().find_many(query).await?;             // SearchResult<T> (paginated)
let total = snugom.guilds().count().await?;                       // u64 (all)
let total = snugom.guilds().count_where(query).await?;            // u64 (filtered)
let exists = snugom.guilds().exists_where(query).await?;          // bool

// ============ Single Record Writes ============
let guild = snugom.guilds().create(entity).await?;                // T (returns created entity)
let guild = snugom.guilds().update(&id, patch).await?;            // T (returns updated entity)
snugom.guilds().delete(&id).await?;                               // ()

// ============ Bulk Operations ============
let result = snugom.guilds().create_many(entities).await?;        // BulkCreateResult { count, ids }
let count = snugom.guilds().update_many(query, patch).await?;     // u64 (count updated)
let count = snugom.guilds().delete_many(query).await?;            // u64 (count deleted)
```

### Prisma Comparison

| Prisma                                 | SnugOM                           | Returns            |
| -------------------------------------- | -------------------------------- | ------------------ |
| `findUnique(where)`                    | `get(id)`                        | `Option<T>`        |
| `findUniqueOrThrow(where)`             | `get_or_error(id)`               | `T`                |
| `findFirst(where)`                     | `find_first(query)`              | `Option<T>`        |
| `findFirstOrThrow(where)`              | `find_first_or_error(query)`     | `T`                |
| `findMany(where, orderBy, skip, take)` | `find_many(query)`               | `SearchResult<T>`  |
| `create(data)`                         | `create(entity)`                 | `T`                |
| `createMany(data[])`                   | `create_many(entities)`          | `BulkCreateResult` |
| `update(where, data)`                  | `update(id, patch)`              | `T`                |
| `updateMany(where, data)`              | `update_many(query, patch)`      | `u64`              |
| `delete(where)`                        | `delete(id)`                     | `()`               |
| `deleteMany(where)`                    | `delete_many(query)`             | `u64`              |
| `count(where)`                         | `count()` / `count_where(query)` | `u64`              |

### Complex Operations (Macro DSL)

Nested creates:

```rust
let guild = snugom.create! {
    Guild {
        name: "Dragon Knights",
        members: [
            create GuildMember { user_id: "u1", role: Role::Leader },
            create GuildMember { user_id: "u2", role: Role::Member },
        ],
    }
}.await?;
```

Two-level deep nesting:

```rust
let guild = snugom.create! {
    Guild {
        name: "Dragon Knights",
        members: [
            create GuildMember {
                user_id: "u1",
                role: Role::Leader,
                achievements: [
                    create MemberAchievement { name: "Founder", earned_at: now },
                ],
            },
        ],
    }
}.await?;
```

Relation mutations in update:

```rust
snugom.update! {
    Guild(id = &guild_id) {
        name: "New Name",
        members: [
            connect new_member_id,
            disconnect old_member_id,
            delete removed_member_id,
            create GuildMember { user_id: "u3", role: Role::Member },
        ],
    }
}.await?;
```

Upsert:

```rust
snugom.upsert! {
    Guild(id = &guild_id) {
        update: {
            member_count: member_count + 1,
        },
        create: Guild {
            name: "New Guild",
            member_count: 1,
        },
    }
}.await?;
```

Delete with cascade/relation cleanup:

```rust
// Simple delete - direct method
snugom.guilds().delete(&id).await?;

// Complex delete with explicit cascade behavior
snugom.delete! {
    Guild(id = &guild_id) {
        members: cascade,      // Delete all members
        applications: cascade, // Delete all applications
    }
}.await?;
```

N-level deep nesting (arbitrary depth supported):

```rust
let org = snugom.create! {
    Organization {
        name: "Acme Corp",
        departments: [
            create Department {
                name: "Engineering",
                teams: [
                    create Team {
                        name: "Backend",
                        members: [
                            create TeamMember {
                                user_id: "u1",
                                role: Role::Lead,
                                permissions: [
                                    create Permission { scope: "read", resource: "*" },
                                    create Permission { scope: "write", resource: "api" },
                                ],
                            },
                        ],
                    },
                ],
            },
        ],
    }
}.await?;
```

---

## Implementation Phases

---

### Phase 1: Core Client Infrastructure

**1.1 Add Dependencies**

```toml
# crates/snugom/Cargo.toml
[dependencies]
inventory = "0.3.21"
```

**1.2 Create Client Module** (`src/client/mod.rs`)

```rust
mod collection;
mod registration;

pub use collection::CollectionHandle;
pub use registration::EntityRegistration;
```

**1.3 Auto-Registration via Entity Derive**

Update `#[derive(SnugomEntity)]` to auto-register:

```rust
#[derive(SnugomEntity)]
#[snugom(collection = "guilds", service = "guild")]
pub struct Guild {
    #[snugom(id)]
    pub guild_id: String,
    // ...
}

// Generated by derive:
inventory::submit! {
    snugom::client::EntityRegistration {
        type_id: std::any::TypeId::of::<Guild>(),
        type_name: "Guild",
        collection_name: "guilds",
        service_name: "guild",
    }
}
```

No separate `bundle!` call needed - entity registration happens automatically.

**1.4 EntityRegistration** (`src/client/registration.rs`)

```rust
use std::any::TypeId;

/// Metadata for auto-discovered entities
pub struct EntityRegistration {
    pub type_id: TypeId,
    pub type_name: &'static str,
    pub collection_name: &'static str,
    pub service_name: &'static str,
}

inventory::collect!(EntityRegistration);

/// Get all registered entities
pub fn registered_entities() -> impl Iterator<Item = &'static EntityRegistration> {
    inventory::iter::<EntityRegistration>()
}
```

**1.5 SnugomClient Derive Macro** (`snugom-macros/src/client_derive.rs`)

````rust
/// Generates a client struct with typed collection accessors
///
/// Usage:
/// ```rust
/// #[derive(SnugomClient)]
/// #[snugom(prefix = "myapp")]
/// pub struct Snugom;
/// ```
///
/// Discovers all entities registered via entity derive and generates:
/// - `fn guilds(&self) -> CollectionHandle<Guild>`
/// - `fn guild_members(&self) -> CollectionHandle<GuildMember>`
/// - etc.
````

The derive macro will:

1. Scan `inventory::iter::<EntityRegistration>()` at compile time
2. Generate a field for each entity's `Repo<T>`
3. Generate accessor methods returning `CollectionHandle<T>`
4. Generate constructors: `connect()`, `with_connection()`, `with_connection_and_prefix()`

**1.6 CollectionHandle Implementation** (`src/client/collection.rs`)

```rust
use redis::aio::ConnectionManager;
use crate::repository::Repo;

/// Type-safe handle for CRUD operations on a single entity collection
pub struct CollectionHandle<'a, T: SnugomEntity> {
    repo: &'a Repo<T>,
    conn: ConnectionManager,
}

impl<'a, T: SnugomEntity> CollectionHandle<'a, T> {
    pub(crate) fn new(repo: &'a Repo<T>, conn: ConnectionManager) -> Self {
        Self { repo, conn }
    }

    // ============ Single Record by ID ============

    /// Get entity by ID
    pub async fn get(&self, id: &str) -> Result<Option<T>, SnugomError> { todo!() }

    /// Get entity by ID, error if not found
    pub async fn get_or_error(&self, id: &str) -> Result<T, SnugomError> {
        self.get(id).await?.ok_or(SnugomError::NotFound { id: id.to_string() })
    }

    /// Check if entity exists by ID
    pub async fn exists(&self, id: &str) -> Result<bool, SnugomError> { todo!() }

    // ============ Query-based Reads ============

    /// Find first entity matching query
    pub async fn find_first(&self, query: SearchQuery) -> Result<Option<T>, SnugomError> { todo!() }

    /// Find first entity matching query, error if not found
    pub async fn find_first_or_error(&self, query: SearchQuery) -> Result<T, SnugomError> {
        self.find_first(query).await?.ok_or(SnugomError::NoMatch)
    }

    /// Find all entities matching query (paginated)
    pub async fn find_many(&self, query: SearchQuery) -> Result<SearchResult<T>, SnugomError> { todo!() }

    /// Count all entities
    pub async fn count(&self) -> Result<u64, SnugomError> { todo!() }

    /// Count entities matching query
    pub async fn count_where(&self, query: SearchQuery) -> Result<u64, SnugomError> { todo!() }

    /// Check if any entity matches query
    pub async fn exists_where(&self, query: SearchQuery) -> Result<bool, SnugomError> { todo!() }

    // ============ Single Record Writes ============

    /// Create entity, returns created entity
    pub async fn create(&self, entity: T) -> Result<T, SnugomError> { todo!() }

    /// Update entity by ID, returns updated entity
    pub async fn update(&self, id: &str, patch: T::Patch) -> Result<T, SnugomError> { todo!() }

    /// Delete entity by ID
    pub async fn delete(&self, id: &str) -> Result<(), SnugomError> { todo!() }

    // ============ Bulk Operations ============

    /// Create multiple entities
    pub async fn create_many(&self, entities: Vec<T>) -> Result<BulkCreateResult, SnugomError> { todo!() }

    /// Update all entities matching query
    pub async fn update_many(&self, query: SearchQuery, patch: T::Patch) -> Result<u64, SnugomError> { todo!() }

    /// Delete all entities matching query
    pub async fn delete_many(&self, query: SearchQuery) -> Result<u64, SnugomError> { todo!() }
}

/// Result of bulk create operation
pub struct BulkCreateResult {
    pub count: u64,
    pub ids: Vec<String>,
}
```

**1.7 Generated Client Structure**

```rust
// Input:
#[derive(SnugomClient)]
#[snugom(prefix = "myapp")]
pub struct Snugom;

// Generated:
pub struct Snugom {
    conn: ConnectionManager,
    prefix: String,
    repo_guild: Repo<Guild>,
    repo_guild_member: Repo<GuildMember>,
    repo_guild_application: Repo<GuildApplication>,
}

impl Snugom {
    /// Create client with a new connection
    pub async fn connect(redis_url: &str) -> Result<Self, SnugomError> {
        let client = redis::Client::open(redis_url)?;
        let conn = client.get_connection_manager().await?;
        Self::with_connection(conn)
    }

    /// Create client with an existing ConnectionManager
    pub fn with_connection(conn: ConnectionManager) -> Result<Self, SnugomError> {
        let prefix = "myapp".to_string();
        Ok(Self {
            conn,
            prefix: prefix.clone(),
            repo_guild: Repo::new(prefix.clone()),
            repo_guild_member: Repo::new(prefix.clone()),
            repo_guild_application: Repo::new(prefix.clone()),
        })
    }

    /// Create client with existing connection and custom prefix
    pub fn with_connection_and_prefix(conn: ConnectionManager, prefix: impl Into<String>) -> Result<Self, SnugomError> {
        let prefix = prefix.into();
        Ok(Self {
            conn,
            prefix: prefix.clone(),
            repo_guild: Repo::new(prefix.clone()),
            repo_guild_member: Repo::new(prefix.clone()),
            repo_guild_application: Repo::new(prefix.clone()),
        })
    }

    pub fn guilds(&self) -> CollectionHandle<'_, Guild> {
        CollectionHandle::new(&self.repo_guild, self.conn.clone())
    }

    pub fn guild_members(&self) -> CollectionHandle<'_, GuildMember> {
        CollectionHandle::new(&self.repo_guild_member, self.conn.clone())
    }

    pub fn guild_applications(&self) -> CollectionHandle<'_, GuildApplication> {
        CollectionHandle::new(&self.repo_guild_application, self.conn.clone())
    }

    /// Ensure all search indexes exist
    pub async fn ensure_indexes(&self) -> Result<(), SnugomError> {
        // Auto-discovered from registered entities
        todo!()
    }
}
```

---

### Phase 2: Macro DSL for Complex Operations

**2.1 Client-Aware Macros**

The key insight: macros take the client as first argument, accessing its connection internally.

```rust
// snugom.create! { ... } expands to use snugom's internal connection
let guild = snugom.create! {
    Guild {
        name: "Test",
        members: [
            create GuildMember { user_id: "u1" },
        ],
    }
}.await?;
```

**2.2 Macro Implementation**

```rust
/// Create with nested relations
#[macro_export]
macro_rules! create {
    ($client:ident, $($body:tt)*) => {{
        // Parse body into MutationPlan
        // Execute using $client's internal connection
        // Return created entity
        $crate::runtime::execute_create(&$client.conn(), &$client.repos(), /* parsed plan */)
    }};
}

/// Update with relation mutations
#[macro_export]
macro_rules! update {
    ($client:ident, $($body:tt)*) => {{
        $crate::runtime::execute_update(&$client.conn(), &$client.repos(), /* parsed plan */)
    }};
}

/// Upsert
#[macro_export]
macro_rules! upsert {
    ($client:ident, $($body:tt)*) => {{
        $crate::runtime::execute_upsert(&$client.conn(), &$client.repos(), /* parsed plan */)
    }};
}

/// Delete with cascade/relation control
#[macro_export]
macro_rules! delete {
    ($client:ident, $($body:tt)*) => {{
        $crate::runtime::execute_delete(&$client.conn(), &$client.repos(), /* parsed plan */)
    }};
}
```

**2.3 N-Level Deep Nesting**

The macro parser handles arbitrary nesting depth by recursively parsing nested `create` blocks. The MutationPlan structure already supports this.

```rust
// 3-level deep example
snugom.create! {
    Guild {
        name: "Test",
        members: [
            create GuildMember {
                user_id: "u1",
                achievements: [
                    create Achievement {
                        name: "Founder",
                        rewards: [
                            create Reward { item_id: "badge_001", quantity: 1 },
                        ],
                    },
                ],
            },
        ],
    }
}.await?;
```

**2.4 Delete Macro**

For simple deletes, use the direct method. For complex deletes with cascade control:

```rust
// Delete with explicit cascade behavior per relation
snugom.delete! {
    Guild(id = &guild_id) {
        members: cascade,       // Delete all related members
        applications: orphan,   // Remove FK but keep records
        audit_logs: restrict,   // Fail if any exist
    }
}.await?;
```

---

### Phase 3: Guild Vertical Slice Migration

**3.1 Create SnugomClient in snug-api**

```rust
// crates/snug-api/src/snugom_client.rs
use snugom::SnugomClient;

#[derive(SnugomClient)]
#[snugom(prefix_fn = "crate::keys::prefix")]
pub struct Snugom;
```

**3.2 Add Snugom to AppState**

```rust
// crates/snug-api/src/state.rs
pub struct AppState {
    // ... existing fields
    pub snugom: Snugom,
}

impl AppState {
    pub async fn new(config: Config) -> Result<Self, ApiError> {
        let snugom = Snugom::connect(&config.core.redis_url).await?;
        snugom.ensure_indexes().await?;
        // ...
    }
}
```

**3.3 Migrate GuildManager**

Before:

```rust
pub struct GuildManager {
    config: GuildConfig,
    repo: Repo<Guild>,
    pub(crate) conn: ConnectionManager,
}

impl GuildManager {
    pub async fn create_guild(&self, name: &str, created_by: &str, ...) -> GuildResult<Guild> {
        let mut conn = self.conn.clone();
        let created = snugom::run! {
            &self.repo, &mut conn,
            create => Guild {
                name: name,
                guild_members: [
                    create GuildMember { user_id: created_by, role: MemberRole::Leader, ... }
                ],
            }
        }.await?;
        // ...
    }
}
```

After:

```rust
pub struct GuildManager {
    config: GuildConfig,
    snugom: Snugom,
}

impl GuildManager {
    pub fn new(snugom: Snugom, config: GuildConfig) -> Self {
        Self { config, snugom }
    }

    pub async fn create_guild(&self, name: &str, created_by: &str, ...) -> GuildResult<Guild> {
        validate_name(name, self.config.max_name_length)?;

        let guild_count = self.snugom.guilds().count().await? as usize;
        if guild_count >= self.config.max_guilds {
            return Err(GuildError::MaxGuildsReached(self.config.max_guilds));
        }

        // Complex nested create - use macro DSL
        let guild = self.snugom.create! {
            Guild {
                name: name,
                visibility: visibility.unwrap_or_default(),
                join_policy: join_policy.unwrap_or_default(),
                member_count: 1,
                max_members: self.config.max_members,
                level: 1,
                xp: 0,
                created_by: created_by,
                metadata: json!({}),
                members: [
                    create GuildMember {
                        user_id: created_by,
                        role: MemberRole::Leader,
                        joined_at: Utc::now(),
                        xp_contributed: 0,
                        metadata: json!({}),
                    },
                ],
            }
        }.await?;

        Ok(guild)
    }

    pub async fn get_guild(&self, guild_id: &str) -> GuildResult<Guild> {
        // Simple read - get_or_error handles not found
        self.snugom.guilds()
            .get_or_error(guild_id)
            .await
            .map_err(|e| match e {
                SnugomError::NotFound { .. } => GuildError::NotFound(guild_id.to_string()),
                other => GuildError::Repo(other),
            })
    }

    pub async fn update_guild(&self, guild_id: &str, patch: GuildPatch) -> GuildResult<Guild> {
        // Simple update - returns updated entity
        self.snugom.guilds()
            .update(guild_id, patch)
            .await
            .map_err(|e| match e {
                SnugomError::NotFound { .. } => GuildError::NotFound(guild_id.to_string()),
                other => GuildError::Repo(other),
            })
    }

    pub async fn add_member(&self, guild_id: &str, user_id: &str, role: MemberRole) -> GuildResult<()> {
        // Update with nested create - use macro DSL
        self.snugom.update! {
            Guild(id = guild_id) {
                member_count: member_count + 1,
                members: [
                    create GuildMember {
                        user_id: user_id,
                        role: role,
                        joined_at: Utc::now(),
                        xp_contributed: 0,
                    },
                ],
            }
        }.await?;

        Ok(())
    }

    pub async fn remove_member(&self, guild_id: &str, member_id: &str) -> GuildResult<()> {
        // Update with disconnect
        self.snugom.update! {
            Guild(id = guild_id) {
                member_count: member_count - 1,
                members: [
                    delete member_id,
                ],
            }
        }.await?;

        Ok(())
    }

    pub async fn delete_guild(&self, guild_id: &str) -> GuildResult<()> {
        // Simple delete - direct method
        self.snugom.guilds().delete(guild_id).await?;
        Ok(())
    }

    pub async fn search_guilds(&self, query: SearchQuery) -> GuildResult<SearchResult<Guild>> {
        // Simple search - direct method
        self.snugom.guilds().find_many(query).await.map_err(GuildError::from)
    }
}
```

**3.4 Migrate GuildMembershipManager**

Similar pattern - replace `repo` + `conn` with `snugom` client.

---

### Phase 4: Testing

**4.1 Unit Tests** (`src/client/tests.rs`)

- CollectionHandle simple CRUD operations
- Entity auto-registration
- Macro DSL parsing

**4.2 Integration Tests** (`tests/client_integration_test.rs`)

```rust
#[tokio::test]
async fn test_simple_crud() {
    let snugom = test_snugom_client().await;

    // Create - returns the entity
    let guild = snugom.guilds().create(Guild {
        name: "Test Guild".into(),
        ..Default::default()
    }).await.unwrap();

    assert_eq!(guild.name, "Test Guild");

    // Read
    let fetched = snugom.guilds().get(&guild.guild_id).await.unwrap().unwrap();
    assert_eq!(fetched.name, "Test Guild");

    // Read with error on not found
    let fetched = snugom.guilds().get_or_error(&guild.guild_id).await.unwrap();
    assert_eq!(fetched.name, "Test Guild");

    // Update - returns updated entity
    let updated = snugom.guilds().update(&guild.guild_id, GuildPatch {
        name: Some("Updated".into()),
        ..Default::default()
    }).await.unwrap();
    assert_eq!(updated.name, "Updated");

    // Delete
    snugom.guilds().delete(&guild.guild_id).await.unwrap();
    assert!(snugom.guilds().get(&guild.guild_id).await.unwrap().is_none());
}

#[tokio::test]
async fn test_query_operations() {
    let snugom = test_snugom_client().await;

    // Create some guilds
    snugom.guilds().create(Guild { name: "Alpha".into(), visibility: Visibility::Public, ..Default::default() }).await.unwrap();
    snugom.guilds().create(Guild { name: "Beta".into(), visibility: Visibility::Public, ..Default::default() }).await.unwrap();
    snugom.guilds().create(Guild { name: "Gamma".into(), visibility: Visibility::Private, ..Default::default() }).await.unwrap();

    // Count all
    assert_eq!(snugom.guilds().count().await.unwrap(), 3);

    // Count with filter
    let query = SearchQuery::new().filter("visibility:eq:public");
    assert_eq!(snugom.guilds().count_where(query.clone()).await.unwrap(), 2);

    // Find first
    let first = snugom.guilds().find_first(query.clone()).await.unwrap();
    assert!(first.is_some());

    // Find many
    let results = snugom.guilds().find_many(query).await.unwrap();
    assert_eq!(results.total, 2);

    // Exists
    assert!(snugom.guilds().exists_where(SearchQuery::new().filter("name:eq:Alpha")).await.unwrap());
}

#[tokio::test]
async fn test_bulk_operations() {
    let snugom = test_snugom_client().await;

    // Bulk create
    let result = snugom.guilds().create_many(vec![
        Guild { name: "One".into(), ..Default::default() },
        Guild { name: "Two".into(), ..Default::default() },
        Guild { name: "Three".into(), ..Default::default() },
    ]).await.unwrap();
    assert_eq!(result.count, 3);

    // Bulk update
    let updated = snugom.guilds().update_many(
        SearchQuery::new().filter("visibility:eq:public"),
        GuildPatch { description: Some("Updated".into()), ..Default::default() }
    ).await.unwrap();

    // Bulk delete
    let deleted = snugom.guilds().delete_many(
        SearchQuery::new().filter("name:eq:One")
    ).await.unwrap();
    assert_eq!(deleted, 1);
}

#[tokio::test]
async fn test_nested_create() {
    let snugom = test_snugom_client().await;

    let guild = snugom.create! {
        Guild {
            name: "Test Guild",
            members: [
                create GuildMember { user_id: "u1", role: MemberRole::Leader },
                create GuildMember { user_id: "u2", role: MemberRole::Member },
            ],
        }
    }.await.unwrap();

    assert_eq!(guild.name, "Test Guild");
    assert_eq!(guild.member_count, 2);

    // Verify members were created
    let members = snugom.guild_members()
        .search(SearchQuery::filter("guild_id", &guild.guild_id))
        .await
        .unwrap();
    assert_eq!(members.total, 2);
}

#[tokio::test]
async fn test_two_level_nesting() {
    let snugom = test_snugom_client().await;

    let guild = snugom.create! {
        Guild {
            name: "Test",
            members: [
                create GuildMember {
                    user_id: "u1",
                    achievements: [
                        create MemberAchievement { name: "Founder" },
                    ],
                },
            ],
        }
    }.await.unwrap();

    // Verify nested achievements
    let achievements = snugom.member_achievements()
        .search(SearchQuery::filter("member_id", /* member id */))
        .await
        .unwrap();
    assert_eq!(achievements.total, 1);
}

#[tokio::test]
async fn test_update_with_relations() {
    let snugom = test_snugom_client().await;

    // Create guild with one member
    let guild = snugom.create! {
        Guild {
            name: "Test",
            members: [
                create GuildMember { user_id: "leader", role: MemberRole::Leader },
            ],
        }
    }.await.unwrap();

    // Add member via update
    snugom.update! {
        Guild(id = &guild.guild_id) {
            name: "Updated Name",
            members: [
                create GuildMember { user_id: "new_member", role: MemberRole::Member },
            ],
        }
    }.await.unwrap();

    let updated = snugom.guilds().get(&guild.guild_id).await.unwrap().unwrap();
    assert_eq!(updated.name, "Updated Name");
    assert_eq!(updated.member_count, 2);
}

#[tokio::test]
async fn test_delete_with_cascade() {
    let snugom = test_snugom_client().await;

    // Create guild with members
    let guild = snugom.create! {
        Guild {
            name: "Test",
            members: [
                create GuildMember { user_id: "u1", role: MemberRole::Leader },
                create GuildMember { user_id: "u2", role: MemberRole::Member },
            ],
        }
    }.await.unwrap();

    // Delete with cascade - should delete members too
    snugom.delete! {
        Guild(id = &guild.guild_id) {
            members: cascade,
        }
    }.await.unwrap();

    // Verify guild is gone
    assert!(snugom.guilds().get(&guild.guild_id).await.unwrap().is_none());

    // Verify members are gone
    let members = snugom.guild_members()
        .search(SearchQuery::filter("guild_id", &guild.guild_id))
        .await
        .unwrap();
    assert_eq!(members.total, 0);
}
```

---

### Phase 5: Documentation & Cleanup

**5.1 Update README**

````markdown
## SnugomClient API

### Setup

```rust
#[derive(SnugomClient)]
#[snugom(prefix = "myapp")]
pub struct Snugom;

let snugom = Snugom::connect("redis://localhost").await?;
```
````

### Simple CRUD

```rust
// Create
let guild = snugom.guilds().create(Guild { name: "Knights".into(), ..Default::default() }).await?;

// Read
let guild = snugom.guilds().get(&id).await?;           // Option<T>
let guild = snugom.guilds().get_or_error(&id).await?;  // T (errors if not found)

// Update
let guild = snugom.guilds().update(&id, GuildPatch { name: Some("New".into()), ..Default::default() }).await?;

// Delete
snugom.guilds().delete(&id).await?;

// Query
let guild = snugom.guilds().find_first(query).await?;  // Option<T>
let guilds = snugom.guilds().find_many(query).await?;  // SearchResult<T>
let count = snugom.guilds().count_where(query).await?; // u64

// Bulk
let result = snugom.guilds().create_many(entities).await?;      // BulkCreateResult
let count = snugom.guilds().update_many(query, patch).await?;   // u64
let count = snugom.guilds().delete_many(query).await?;          // u64
```

### Complex Operations (Macro DSL)

```rust
// Nested create (supports n-level deep)
let guild = snugom.create! {
    Guild {
        name: "Dragon Knights",
        members: [
            create GuildMember { user_id: "u1", role: Role::Leader },
            create GuildMember { user_id: "u2", role: Role::Member },
        ],
    }
}.await?;

// Update with relation mutations
snugom.update! {
    Guild(id = &guild_id) {
        name: "New Name",
        members: [
            connect new_member_id,
            disconnect old_member_id,
            delete removed_member_id,
        ],
    }
}.await?;

// Delete with cascade control
snugom.delete! {
    Guild(id = &guild_id) {
        members: cascade,
        applications: cascade,
    }
}.await?;
```

````

**5.2 Remove bundle! Completely**

With auto-registration from entity derive, `bundle!` is completely removed. Index management is handled automatically by `SnugomClient::ensure_indexes()` which discovers all registered entities.

**5.3 Deprecation Notices**

```rust
/// DEPRECATED: Use SnugomClient pattern instead.
#[deprecated(since = "0.2.0", note = "Use SnugomClient with snugom.guilds().create() or snugom.create! { }")]
pub fn run!(...) { ... }
````

---

## File Structure

```
crates/snugom/
├── src/
│   ├── client/
│   │   ├── mod.rs              # Module exports
│   │   ├── collection.rs       # CollectionHandle<T>
│   │   └── registration.rs     # Entity auto-registration
│   ├── macros/
│   │   ├── create.rs           # snugom.create! { }
│   │   ├── update.rs           # snugom.update! { }
│   │   ├── upsert.rs           # snugom.upsert! { }
│   │   └── delete.rs           # snugom.delete! { }
│   └── lib.rs                  # Add client module

crates/snugom-macros/
├── src/
│   ├── client_derive.rs        # SnugomClient derive macro
│   ├── entity_derive.rs        # Updated to auto-register
│   └── lib.rs                  # Export SnugomClient derive

crates/snug-api/
├── src/
│   ├── snugom_client.rs        # App's SnugomClient definition
│   ├── guild/
│   │   └── manager.rs          # Migrated to use Snugom client
│   └── state.rs                # Add Snugom to AppState
```

---

## Migration Checklist

### Phase 1: Core Infrastructure ✓

- [x] Add `inventory = "0.3.21"` dependency
- [x] Implement `EntityRegistration` struct
- [x] Update entity derive to auto-register with inventory
- [x] Implement `CollectionHandle<T>`:
  - [x] Single record by ID: `get`, `get_or_error`, `exists`
  - [x] Query-based reads: `find_first`, `find_first_or_error`, `find_many`, `count`, `count_where`, `exists_where`
  - [x] Single record writes: `create`, `update`, `delete`, `create_and_get`
  - [x] Bulk operations: `create_many`, `update_many`, `delete_many`
- [x] Implement `SnugomClient` derive macro
- [x] Support `prefix`, `default_prefix`, and `prefix_fn` attributes
- [x] Support `with_connection()` and `with_connection_and_prefix()` constructors
- [x] Implement `BulkCreateResult` struct
- [x] Basic unit tests

### Phase 2: Macro DSL ✓

- [x] Implement `snugom_create! { }` macro
- [x] Implement `snugom_update! { }` macro
- [x] Implement `snugom_upsert! { }` macro
- [x] Implement `snugom_delete! { }` macro
- [x] Support n-level deep nesting (recursive parsing)
- [x] Integration with existing MutationPlan/runtime execution

### Phase 2.5: Macro Ergonomics - Borrowing Instead of Moving ✓

The macros originally used `async move { ... }` which consumed all captured variables, forcing users to defensively clone everything before passing to macros. This is unergonomic for the 99% use case where the future is immediately awaited.

**The Fix:** Change from `async move` to `async` in all macro emit functions:

- Values are borrowed during the await instead of moved
- After the await completes, original values remain available
- No more defensive cloning needed at call sites

**Files updated:**

- [x] `snugom-macros/src/client_ops_macro.rs` - Changed `async move` to `async` in:
  - `ClientCreateInvocation::emit()`
  - `ClientUpdateInvocation::emit()`
  - `ClientDeleteInvocation::emit()` (was already using `async`)
  - `ClientUpsertInvocation::emit()`
- [x] `snugom/tests/client_integration.rs` - Removed defensive cloning in tests
- [x] `snug-api/src/distributed_state/manager.rs` - Simplified cloning pattern

### Phase 2.6: Centralized Index Management ✓ (for migrated managers)

The `SnugomClient` derive macro already generates an `ensure_indexes()` method that knows about ALL registered entities. Currently, every manager has its own `ensure_search_indexes()` method called in `initialize()`. This is redundant.

**The Simplification:**

- [x] `SnugomClient` already has `ensure_indexes()` method (from derive macro)
- [x] Call `snugom_client.ensure_indexes()` once in `main.rs` after client creation
- [x] Remove `ensure_search_indexes()` from migrated managers (Guild, Auction, Lottery, DistributedState)
- [x] Simplify migrated manager `initialize()` methods to just create the manager (sync, returns Arc)
- [x] Update tests to call `ensure_indexes()` once in setup (tests/common/mod.rs, lib.rs test helper)

**All managers migrated and using centralized index management:**

- [x] FSM ✓
- [x] HITL ✓
- [x] Timeline ✓
- [x] KV ✓
- [x] Loyalty ✓
- [x] Matchmaking ✓
- [x] MessageQueue ✓
- [x] RateLimit ✓
- [x] RemoteConfig ✓
- [x] SurgePricing ✓
- [x] Tournament ✓
- [x] WaitingRoom ✓
- [x] Webhook ✓

### Phase 3: Guild Migration ✓

- [x] Create `SnugomClient` in snug-api (`src/snugom_client.rs`)
- [x] Add `SnugomClient` to `Services` in `services.rs`
- [x] Migrate `GuildManager` to use `SnugomClient`
- [x] Migrate `GuildMembershipManager` to use `SnugomClient`
- [x] Update handlers to use new managers
- [x] Verify all existing tests pass
- [x] Add parity tests

### Phase 3.5: Migrate all other snug-api services ✓

All managers migrated to use `Arc<SnugomClient>` pattern:

- [x] Migrate Achievement ✓
- [x] Migrate Auction ✓
- [x] Migrate DistributedState ✓
- [x] Migrate Fsm ✓
- [x] Migrate Hitl ✓
- [x] Migrate Kv ✓
- [x] Migrate Lottery ✓
- [x] Migrate Loyalty ✓
- [x] Migrate Matchmaking ✓
- [x] Migrate MessageQueue ✓ (all sub-managers: dlq, publish, queue, receive, subscription, topic, visibility)
- [x] Migrate RateLimit ✓
- [x] Migrate RemoteConfig ✓
- [x] Migrate SurgePricing ✓
- [x] Migrate Timeline ✓
- [x] Migrate Tournament ✓
- [x] Migrate WaitingRoom ✓
- [x] Migrate Webhook ✓

Services not requiring migration (no SnugOM entities):

- N/A Auth (uses JWT validation, no storage)
- N/A Blob (uses Garage S3-compatible storage, no Redis entities)
- N/A DistributedCircuitBreaker (uses raw Redis, no entities)
- N/A JobQueue (uses raw Redis streams)
- N/A Leaderboard (uses raw Redis sorted sets)
- N/A LiveJson (uses raw Redis pub/sub)
- N/A Liveness (health check only)
- N/A PubSub (uses raw Redis pub/sub)
- N/A WebSocket (connection management, no entities)

Services migrated after initial plan:

- [x] Geo (GeoIndex entity added - uses snugom for index metadata, Redis GEO for spatial data)

### Phase 4: Testing ✓

- [x] Unit tests for CollectionHandle
- [x] Unit tests for macro DSL
- [x] Integration tests (30 tests in client_integration.rs)
- [x] Integration tests for Guild migration
- [x] All 1249 library tests pass
- [x] All snugom tests pass
- [ ] Performance benchmarks (optional)

### Phase 5: Documentation ✓

- [x] Update snugom README with Prisma-style API docs
- [x] Update plan document to reflect completed phases
- [x] Update examples in snugom/src/examples/

---

## Success Criteria

1. **Guild service fully migrated** - All Guild operations use new client API
2. **Tests passing** - All existing tests continue to pass
3. **No performance regression** - Benchmarks show equivalent or better performance
4. **API ergonomics improved** - Simple ops are simple, complex ops use clean DSL
5. **Auto-discovery works** - Entity registration happens automatically from derive
6. **Documentation complete** - README shows new patterns clearly

---

## Risk Mitigation

| Risk                       | Mitigation                                                    |
| -------------------------- | ------------------------------------------------------------- |
| `inventory` crate issues   | Well-maintained crate, used by major projects (tracing, etc.) |
| Macro DSL complexity       | Reuse existing `run!` macro parser, extensive tests           |
| N-level nesting edge cases | Comprehensive test coverage, recursive parser already exists  |
| Performance regression     | Benchmark before/after                                        |
| Breaking existing code     | Deprecation warnings, migration guide                         |

---

## Timeline Estimates

| Phase   | Effort   | Dependencies |
| ------- | -------- | ------------ |
| Phase 1 | 4-5 days | None         |
| Phase 2 | 3-4 days | Phase 1      |
| Phase 3 | 2-3 days | Phase 1-2    |
| Phase 4 | 2-3 days | Phase 1-3    |
| Phase 5 | 1-2 days | Phase 1-4    |

**Total: ~12-17 days**

---

## Future Extensions

After Guild migration is stable:

1. ✅ **Migrate remaining services** - All services migrated to SnugomClient
2. **Transaction support** - `snugom.transaction(|tx| { ... })`
3. ✅ **Batch operations** - `snugom.guilds().create_many(vec![...])` implemented
4. **Streaming results** - `snugom.guilds().search_stream(query)`
5. ✅ **Remove legacy code** - Deleted old `run!` macro (replaced with `snugom_create!`, `snugom_update!`, `snugom_delete!`, `snugom_upsert!`)
