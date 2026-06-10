# SnugomClient vs PrismaClient: Atomicity Comparison

**Status:** Research Document
**Created:** 2025-01-01

## Executive Summary

SnugomClient provides **strong single-entity atomicity** via Lua scripts for Redis, but has **gaps in multi-entity operations** compared to Prisma's SQL transaction-backed atomicity.

---

## Prisma's Atomicity Model (SQL-backed)

Prisma's operations are backed by SQL transactions, providing ACID guarantees:

| Operation | Atomicity Guarantee |
|-----------|---------------------|
| `create()` | Atomic (single INSERT) |
| `update()` | Atomic (single UPDATE) |
| `delete()` | Atomic (single DELETE) |
| `upsert()` | Atomic (INSERT ON CONFLICT) |
| `createMany()` | Atomic (single transaction) |
| `updateMany()` | Atomic (single UPDATE WHERE) |
| `deleteMany()` | Atomic (single DELETE WHERE) |
| Nested creates | Atomic (wrapped in transaction) |
| Nested mutations | Atomic (wrapped in transaction) |
| `$transaction([...])` | Atomic (explicit transaction) |

**Key SQL features enabling this:**
- `BEGIN`/`COMMIT`/`ROLLBACK` - Multi-statement transactions
- `INSERT ON CONFLICT` - Atomic upsert
- `UPDATE ... WHERE` - Bulk atomic updates
- FK constraints with CASCADE - Atomic cascading deletes

---

## SnugomClient's Atomicity Model (Redis + Lua)

### Atomic Operations (via Lua scripts)

| Operation | Implementation | Atomicity |
|-----------|----------------|-----------|
| `create()` | `entity_mutation.lua` | Fully atomic |
| `update()` | `entity_patch.lua` | Fully atomic |
| `delete()` | `entity_delete.lua` | Fully atomic |
| `upsert()` | `entity_upsert.lua` | Fully atomic |
| Relation mutations | Within Lua script | Atomic with parent |

**What Lua scripts guarantee atomically:**
1. Version checking + mutation (optimistic concurrency)
2. Existence check + create/update (upsert)
3. Unique constraint validation + entity write
4. Entity write + relation set mutations
5. Entity write + datetime mirrors
6. Idempotency check + operation

### Non-Atomic Operations

| Operation | Implementation | Issue |
|-----------|----------------|-------|
| `create_many()` | Loop in `collection.rs` | Each create is separate Lua call |
| `update_many_by_ids()` | Loop in `collection.rs` | Each update is separate Lua call |
| `delete_many_by_ids()` | Loop in `collection.rs` | Each delete is separate Lua call |
| `delete_many(query)` | Search + loop | Search then delete loop |
| `update_many(query)` | Search + loop | Search then update loop |
| Nested creates (`snugom_create!`) | Sequential Lua calls | Parent created, then children |

---

## Detailed Gap Analysis

### Gap 1: Bulk Operations Are Not Transactional

**Prisma:**
```typescript
// ATOMIC - all or nothing
await prisma.user.createMany({
  data: [user1, user2, user3]
})
```

**Snugom:**
```rust
// NOT ATOMIC - if user2 fails, user1 is already committed
snugom.guilds().create_many(vec![guild1, guild2, guild3]).await?;
```

**Impact:** Partial commits possible. If `guild2` fails (e.g., unique constraint violation), `guild1` remains in Redis.

### Gap 2: Nested Creates Are Not Atomic

**Prisma:**
```typescript
// ATOMIC - guild + members in single transaction
await prisma.guild.create({
  data: {
    name: "Knights",
    members: {
      create: [{ userId: "u1" }, { userId: "u2" }]
    }
  }
})
```

**Snugom:**
```rust
// NOT ATOMIC - guild created first, then members via separate Lua calls
snugom_create!(&client, &mut conn, Guild {
    name: "Knights",
    members: [
        create GuildMember { user_id: "u1" },
        create GuildMember { user_id: "u2" },
    ],
}).await?;
```

**How snugom_create! works internally:**
1. Guild created via `entity_mutation.lua` (atomic)
2. GuildMember 1 created via `entity_mutation.lua` (atomic)
3. GuildMember 2 created via `entity_mutation.lua` (atomic)
4. Relation sets updated

**Impact:** If member 2 creation fails, guild and member 1 are committed.

### Gap 3: Query-Based Bulk Updates Have Race Window

**Prisma:**
```typescript
// ATOMIC - WHERE evaluated and UPDATE applied in single statement
await prisma.user.updateMany({
  where: { status: 'pending' },
  data: { status: 'active' }
})
```

**Snugom:**
```rust
// NOT ATOMIC - search happens, then loop through results
snugom.users().update_many(
    SearchQuery::new().filter("status:eq:pending"),
    |id| User::patch_builder().entity_id(id).status("active")
).await?;
```

**Flow:**
1. Search for matching entities (returns N IDs)
2. Loop through IDs, call `entity_patch.lua` for each
3. New entities matching query created *during* loop won't be updated

**Impact:**
- Race condition: new matching entities not updated
- Partial failure: some updated, some not

### Gap 4: No Multi-Entity Transaction Support

**Prisma:**
```typescript
// ATOMIC - all operations succeed or none do
await prisma.$transaction([
  prisma.account.update({ where: { id: from }, data: { balance: { decrement: 100 } } }),
  prisma.account.update({ where: { id: to }, data: { balance: { increment: 100 } } }),
])
```

**Snugom:** No equivalent. Each Lua script is independent.

### Gap 5: Existence Check Before Create Has Race Window

In `repository/mod.rs`, `create_with_conn()` performs an existence check before the Lua script:

```rust
if self.exists(&mut conn, id).await? {
    return Err(RepoError::AlreadyExists);
}
// Another process could create the entity here!
self.execute_mutation_lua(...).await
```

**Impact:** Two concurrent creates with same ID could both pass existence check.

**Note:** The Lua script itself has idempotency handling, but the pre-check is non-atomic.

---

## What SnugomClient Does Well

### 1. Single Entity Atomicity
Each individual mutation is fully atomic:
- Unique constraints checked and set atomically
- Version incremented atomically
- Relations mutated atomically with entity
- Datetime mirrors applied atomically

### 2. Optimistic Concurrency
`expected_version` parameter enables safe concurrent updates:
```rust
snugom.guilds().update(
    Guild::patch_builder()
        .entity_id(id)
        .expected_version(5)
        .name("New Name")
).await?;
// Fails with version_conflict if version != 5
```

### 3. Idempotency
Operations with `idempotency_key` are safely deduplicated (900s TTL default).

### 4. Atomic Upsert
Unlike many ORMs, upsert is truly atomic:
```rust
snugom.guilds().upsert(create_builder, update_builder).await?;
// Existence check + create/update in single Lua script
```

---

## Recommended Mitigations

### Option 1: Redis Transactions (MULTI/EXEC)
For bulk operations that must be atomic:
```rust
// Wrap multiple Lua calls in MULTI/EXEC
conn.multi()
    .invoke_script(create_lua, &[...])
    .invoke_script(create_lua, &[...])
    .exec()
    .await?;
```
**Limitation:** Still not true transactions - no rollback on failure.

### Option 2: Single Comprehensive Lua Script
For specific atomic patterns (e.g., `create_with_children`):
```lua
-- entity_create_with_children.lua
-- Atomically create parent + all children
```
**Limitation:** Must be designed per use case.

### Option 3: Saga Pattern
For cross-entity operations that can't be atomic:
```rust
// Create with compensating actions on failure
let guild = create_guild().await?;
match create_members(guild.id).await {
    Ok(_) => Ok(guild),
    Err(e) => {
        delete_guild(guild.id).await?; // Compensate
        Err(e)
    }
}
```

### Option 4: Accept Eventual Consistency
For many use cases, eventual consistency is acceptable:
- Use idempotency keys to safely retry
- Use version checks to detect conflicts
- Implement cleanup jobs for orphaned data

---

## Summary Table

| Feature | Prisma | Snugom | Gap |
|---------|--------|--------|-----|
| Single create | Atomic | Atomic | None |
| Single update | Atomic | Atomic | None |
| Single delete | Atomic | Atomic | None |
| Upsert | Atomic | Atomic | None |
| Unique constraints | Atomic | Atomic | None |
| Version/OCC | Supported | Supported | None |
| Bulk create | Atomic | Loop | **Gap** |
| Bulk update (query) | Atomic | Search + loop | **Gap** |
| Bulk delete (query) | Atomic | Search + loop | **Gap** |
| Nested creates | Atomic | Sequential Lua | **Gap** |
| Explicit transaction | `$transaction()` | None | **Gap** |
| Cascading delete | FK + CASCADE | In Lua, recursive | Partial |
| Rollback on failure | Automatic | None | **Gap** |

---

## Conclusion

The core gaps stem from Redis's lack of multi-key transactions. Lua scripts provide per-operation atomicity, but cannot span multiple independent operations. For most CRUD workloads this is sufficient, but patterns requiring "all-or-nothing" across multiple entities need application-level coordination.

### When SnugomClient's Model Is Sufficient
- Single-entity CRUD operations
- Operations with idempotency requirements
- Workflows tolerant of eventual consistency
- Use cases where optimistic concurrency (version checks) handles conflicts

### When Gaps Matter
- Financial transactions (transfers between accounts)
- Bulk imports that must be all-or-nothing
- Complex nested creates where partial state is invalid
- Multi-entity invariants that must always hold
