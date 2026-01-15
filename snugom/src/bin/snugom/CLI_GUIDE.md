# SnugOM CLI Guide

The `snugom` CLI is a schema versioning and migration tool for SnugOM. It provides automatic schema change detection, migration generation, and deployment to Redis.

## Table of Contents

- [Installation](#installation)
- [Getting Started](#getting-started)
- [Commands Reference](#commands-reference)
  - [snugom init](#snugom-init)
  - [snugom migrate](#snugom-migrate)
  - [snugom schema](#snugom-schema)
- [Migration Complexity Levels](#migration-complexity-levels)
- [Workflows](#workflows)
  - [Initial Project Setup](#initial-project-setup)
  - [Adding a New Entity](#adding-a-new-entity)
  - [Modifying an Existing Entity](#modifying-an-existing-entity)
  - [Adding Unique Constraints](#adding-unique-constraints)
  - [Deploying to Production](#deploying-to-production)
  - [Recovering from Failed Migrations](#recovering-from-failed-migrations)
- [Project Structure](#project-structure)
- [Schema Snapshots](#schema-snapshots)
- [Environment Variables](#environment-variables)
- [Global Options](#global-options)
- [Best Practices](#best-practices)
- [Troubleshooting](#troubleshooting)

---

## Installation

The `snugom` CLI is built as part of the SnugOM crate:

```bash
# Build and install the CLI
cargo install --path cargo/crates/snugom

# Or run directly from the workspace
cargo run -p snugom --bin snugom -- --help
```

---

## Getting Started

### 1. Initialize Your Project

```bash
cd your-rust-project
snugom init
```

This creates the necessary directory structure for migrations:

```
your-project/
├── .snugom/
│   ├── config.toml
│   └── schemas/          # Schema snapshots
└── src/
    └── migrations/
        └── mod.rs        # Migration module registry
```

### 2. Define Your Entities

Create entities using the `#[derive(SnugomEntity)]` macro:

```rust
use snugom::SnugomEntity;
use chrono::{DateTime, Utc};

#[derive(SnugomEntity, serde::Serialize, serde::Deserialize)]
#[snugom(schema = 1)]
pub struct User {
    #[snugom(id)]
    pub user_id: String,

    #[snugom(searchable, filterable(tag))]
    pub email: String,

    #[snugom(searchable)]
    pub name: String,

    #[snugom(datetime, created_at, sortable)]
    pub created_at: DateTime<Utc>,
}
```

### 3. Generate Your First Migration

```bash
snugom migrate create --name init
```

### 4. Deploy the Migration

```bash
# Set your Redis URL
export REDIS_URL="redis://localhost:6379"

# Apply the migration
snugom migrate deploy
```

---

## Commands Reference

### snugom init

Initialize SnugOM in a project directory.

```bash
snugom init [--force]
```

**Options:**

| Option | Description |
|--------|-------------|
| `--force` | Reinitialize and overwrite existing configuration |

**Examples:**

```bash
# Initialize a new project
snugom init

# Reinitialize an existing project (overwrites config)
snugom init --force
```

**What it creates:**

- `.snugom/` directory for configuration and snapshots
- `.snugom/config.toml` with default settings
- `.snugom/schemas/` directory for schema snapshots
- `src/migrations/` directory for migration files
- `src/migrations/mod.rs` with migration registry boilerplate

---

### snugom migrate

Generate and deploy schema migrations.

#### Subcommands

##### `snugom migrate create`

Generate a new migration by detecting schema changes in your entity definitions.

```bash
snugom migrate create --name <name>
```

**Arguments:**

| Argument | Required | Description |
|----------|----------|-------------|
| `--name`, `-n` | Yes | Name for the migration (e.g., `add_avatar`, `split_name`) |

**What it does:**

1. Scans your codebase for `#[derive(SnugomEntity)]` structs
2. Parses each entity's fields, attributes, relations, and indexes
3. Loads existing schema snapshots from `.snugom/schemas/`
4. Computes the diff between current code and snapshots
5. Classifies changes by complexity (BASELINE, AUTO, STUB, COMPLEX)
6. Generates a timestamped migration file in `src/migrations/`
7. Updates `src/migrations/mod.rs` to include the new migration
8. Saves new schema snapshots

**Examples:**

```bash
# Create initial migration for new entities
snugom migrate create --name init

# Create migration after adding a field
snugom migrate create --name add_user_avatar

# Create migration for a specific feature
snugom migrate create --name add_guild_roles
```

**Output:**

```
Generate Migration
  • Migration name: add_avatar

✓ Found 3 file(s) with SnugomEntity
✓ Parsed 3 entity schema(s)
  Found 2 existing snapshot(s)

Detecting Changes
  • User (v1 → v2) - 1 change(s) [AUTO]
      + avatar_url: Option<String>

Generating Migration
✓ Created: _20241228_143000_add_avatar.rs
  • Type: AUTO

Updating Source Files
  • src/models/user.rs: schema 1 → 2

Saving Snapshots
  • Saved: User_v2_20241228_143000.json

Summary
✓ Migration generated successfully!

Next steps:
  • Review the generated migration
  • Commit the changes
  • Run 'snugom migrate deploy' to apply
```

---

##### `snugom migrate deploy`

Run pending migrations against Redis.

```bash
snugom migrate deploy [--dry-run]
```

**Options:**

| Option | Description |
|--------|-------------|
| `--dry-run` | Preview what would be migrated without making changes |

**What it does:**

1. Connects to Redis using `REDIS_URL`
2. Discovers migration files in `src/migrations/`
3. Checks which migrations have already been applied (stored in Redis)
4. Runs each pending migration in order
5. Records successful migrations in `_snugom:migrations` key

**Examples:**

```bash
# Preview migrations (no changes made)
snugom migrate deploy --dry-run

# Apply all pending migrations
snugom migrate deploy
```

**Dry-Run Output:**

```
Deploy Migrations
⚠ DRY RUN MODE - No changes will be made
  • Redis: redis://localhost:6379

✓ Connected to Redis
  Discovering migrations...

✓ Found 2 migration(s)
  Checking applied migrations...
  1 migration(s) pending, 1 already applied

Applying: 20241228_143000_add_avatar
  • Migration type: AUTO
  • Documents: 0 (placeholder)
✓ Applied in 15ms

Summary
✓ 1 migration(s) applied in 42ms
  1 migration(s) already applied
⚠ DRY RUN - No actual changes were made
```

---

##### `snugom migrate resolve`

Manually mark a migration as applied or rolled back. Use this for recovery scenarios when automatic tracking is out of sync.

```bash
snugom migrate resolve <migration_name> --applied|--rolled-back
```

**Arguments:**

| Argument | Required | Description |
|----------|----------|-------------|
| `migration_name` | Yes | Name of the migration to resolve |

**Options (mutually exclusive):**

| Option | Description |
|--------|-------------|
| `--applied` | Mark the migration as applied |
| `--rolled-back` | Mark the migration as rolled back (removes from applied list) |

**Examples:**

```bash
# Mark a migration as applied (e.g., after manual intervention)
snugom migrate resolve 20241228_100000_init --applied

# Mark a migration as rolled back (remove from applied list)
snugom migrate resolve 20241228_143000_add_avatar --rolled-back
```

**Use Cases:**

- **Manual data migration**: You ran the migration logic manually and need to mark it complete
- **Rollback recovery**: A migration failed partway through and you fixed it manually
- **Environment sync**: Aligning migration state between environments
- **Testing**: Resetting migration state for re-testing

---

### snugom schema

View schema status, differences, and validate data.

#### Subcommands

##### `snugom schema status`

Show schema version distribution in Redis. This helps you understand what versions of your entities exist in the database.

```bash
snugom schema status [collection]
```

**Arguments:**

| Argument | Required | Description |
|----------|----------|-------------|
| `collection` | No | Specific collection to check (shows all if omitted) |

**What it does:**

1. Connects to Redis using `REDIS_URL`
2. Scans all documents matching the collection pattern
3. Reads the `__schema_version` field from each document
4. Aggregates and displays version distribution

**Examples:**

```bash
# Show status for all collections
snugom schema status

# Show status for a specific collection
snugom schema status users

# Show status for guild members
snugom schema status guild_members
```

**Output:**

```
Schema Status
✓ Connected to Redis
  Scanning 3 collection(s)...

Collection: users
  • v1: 150 document(s) (30.0%)
  • v2: 350 document(s) (70.0%)

Collection: guilds
  • v1: 50 document(s) (100.0%)

Collection: posts
  ⚠ No version: 25 document(s) (100.0%) - needs migration

Summary
  • Total documents scanned: 575
  • Documents with schema version: 550
  ⚠ Documents without schema version: 25
  Run 'snugom migrate deploy' to apply pending migrations
```

---

##### `snugom schema diff`

Show what changes would be included in the next migration. Use this to preview changes before generating a migration.

```bash
snugom schema diff [entity]
```

**Arguments:**

| Argument | Required | Description |
|----------|----------|-------------|
| `entity` | No | Specific entity to check (shows all if omitted) |

**What it does:**

1. Scans your codebase for entity definitions
2. Loads existing schema snapshots
3. Computes the diff without creating any files
4. Displays pending changes with complexity classification

**Examples:**

```bash
# Show pending changes for all entities
snugom schema diff

# Show pending changes for a specific entity
snugom schema diff User

# Check if a specific entity has changes
snugom schema diff GuildMember
```

**Output:**

```
Schema Diff
✓ Found 3 file(s) with SnugomEntity
✓ Parsed 3 entity schema(s)
  Found 2 existing snapshot(s)

Pending Changes
  • User (v1 → v2) - 2 change(s) [AUTO]
      + field avatar_url: Option<String>
      + index on email
  • GuildMember (NEW) - will be baseline v1
      Source: src/models/guild.rs
    Guild - no changes

Summary
  • 1 new entity/entities
  • 1 modified entity/entities
  Run 'snugom migrate create --name <name>' to generate a migration
```

---

##### `snugom schema validate`

Validate data before adding unique constraints. Checks for duplicate values that would violate uniqueness.

```bash
snugom schema validate <collection> --field <field> [--case-insensitive]
```

**Arguments:**

| Argument | Required | Description |
|----------|----------|-------------|
| `collection` | Yes | Collection to validate |

**Options:**

| Option | Required | Description |
|--------|----------|-------------|
| `--field` | Yes | Field to check for uniqueness |
| `--case-insensitive` | No | Perform case-insensitive duplicate check |

**What it does:**

1. Connects to Redis
2. Scans all documents in the collection
3. Extracts the specified field value from each document
4. Identifies duplicate values
5. Reports which documents have conflicts

**Examples:**

```bash
# Check if email field is unique (case-sensitive)
snugom schema validate users --field email

# Check username uniqueness (case-insensitive)
snugom schema validate users --field username --case-insensitive

# Validate guild names before adding unique constraint
snugom schema validate guilds --field name
```

**Success Output:**

```
Validate Uniqueness: users.email
✓ Connected to Redis
  Checking for duplicate values in 'email'
  • Mode: case-sensitive

Results
✓ No duplicates found! Field 'email' is unique across 500 document(s)
  Safe to add a unique constraint
```

**Failure Output:**

```
Validate Uniqueness: users.email
✓ Connected to Redis
  Checking for duplicate values in 'email'
  • Mode: case-insensitive

Results
✗ Found 3 duplicate value(s) across 8 document(s)

Duplicate Values
  • 1: "john@example.com" appears 3 time(s)
      - users:abc123
      - users:def456
      - users:ghi789
  • 2: "jane@example.com" appears 2 time(s)
      - users:jkl012
      - users:mno345
  • 3: "admin@example.com" appears 3 time(s)
      - users:pqr678
      - users:stu901
      - users:vwx234

Required Action
⚠ You must resolve these duplicates before adding a unique constraint
  Options:
  • Update duplicate values to be unique
  • Delete duplicate documents
  • Choose a different field for the unique constraint
```

---

## Migration Complexity Levels

When generating migrations, the CLI classifies each change by complexity:

| Level | Description | Action Required |
|-------|-------------|-----------------|
| **BASELINE** | New entity with no existing data | None - migration is ready to run |
| **AUTO** | Safe transformation (add optional field, remove field, add index) | None - migration is ready to run |
| **STUB** | Requires custom transformation logic | Review and implement the migration function |
| **COMPLEX** | Multiple breaking changes | Review carefully and implement custom logic |
| **METADATA_ONLY** | Only schema version or attribute changes | None - migration is ready to run |

### AUTO Changes (Safe)

These changes can be applied automatically:

- Adding an optional field (`Option<T>`)
- Adding a field with a default value (String, numbers, bool)
- Removing a field
- Adding or removing an index
- Adding or removing a relation

### STUB Changes (Manual)

These changes require custom migration logic:

- Changing a field's type (e.g., `String` to `i32`)
- Adding a required field without a default value
- Renaming a field (detected as remove + add)
- Complex data transformations

---

## Workflows

### Initial Project Setup

Set up SnugOM migrations in a new project:

```bash
# 1. Navigate to your Rust project
cd my-rust-project

# 2. Initialize SnugOM
snugom init

# 3. Define your entities with #[derive(SnugomEntity)]
# (edit your source files)

# 4. Generate the initial migration
snugom migrate create --name init

# 5. Review the generated migration
cat src/migrations/_*_init.rs

# 6. Set Redis connection
export REDIS_URL="redis://localhost:6379"

# 7. Deploy the migration
snugom migrate deploy
```

---

### Adding a New Entity

Add a completely new entity to your system:

```bash
# 1. Define the new entity in your source code
cat >> src/models/notification.rs << 'EOF'
#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1)]
pub struct Notification {
    #[snugom(id)]
    pub notification_id: String,

    #[snugom(filterable(tag))]
    pub user_id: String,

    #[snugom(searchable)]
    pub message: String,

    #[snugom(filterable)]
    pub read: bool,

    #[snugom(datetime, created_at, sortable)]
    pub created_at: DateTime<Utc>,
}
EOF

# 2. Preview the changes
snugom schema diff Notification

# 3. Generate the migration
snugom migrate create --name add_notifications

# 4. Deploy
snugom migrate deploy
```

---

### Modifying an Existing Entity

Add, modify, or remove fields from an entity:

```bash
# 1. Edit your entity definition
# Example: Add an optional avatar_url field to User

# 2. Preview the diff
snugom schema diff User

# Output:
#   • User (v1 → v2) - 1 change(s) [AUTO]
#       + avatar_url: Option<String>

# 3. Generate the migration
snugom migrate create --name add_user_avatar

# 4. The CLI automatically:
#    - Creates the migration file
#    - Updates src/migrations/mod.rs
#    - Updates the schema version in your source file
#    - Saves a new snapshot

# 5. Test with dry-run
snugom migrate deploy --dry-run

# 6. Apply the migration
snugom migrate deploy
```

---

### Adding Unique Constraints

Safely add a unique constraint to an existing field:

```bash
# 1. First, validate the data has no duplicates
snugom schema validate users --field email

# If duplicates exist, resolve them first:
# - Update duplicate values
# - Delete duplicate documents
# - Or choose a different field

# 2. Once validated, add the unique attribute
# Change: #[snugom(filterable(tag))]
# To:     #[snugom(unique, filterable(tag))]

# 3. Preview the change
snugom schema diff User

# 4. Generate and deploy the migration
snugom migrate create --name add_email_unique
snugom migrate deploy
```

For case-insensitive uniqueness:

```bash
# Validate with case-insensitive check
snugom schema validate users --field username --case-insensitive

# Add case-insensitive unique
# #[snugom(unique(case_insensitive), filterable(tag))]
```

---

### Deploying to Production

Deploy migrations to a production environment:

```bash
# 1. Review all pending migrations locally
snugom schema diff

# 2. Test migrations on staging
export REDIS_URL="redis://staging-redis:6379"
snugom migrate deploy --dry-run
snugom migrate deploy

# 3. Check schema status after deployment
snugom schema status

# 4. Deploy to production
export REDIS_URL="redis://production-redis:6379"

# Always use dry-run first in production
snugom migrate deploy --dry-run

# Apply if dry-run looks correct
snugom migrate deploy

# 5. Verify
snugom schema status
```

---

### Recovering from Failed Migrations

Handle scenarios where a migration fails or needs manual intervention:

#### Scenario 1: Migration Failed Partway Through

```bash
# 1. Fix the underlying issue (data, code, etc.)

# 2. If you fixed the data manually, mark the migration as applied
snugom migrate resolve 20241228_143000_add_avatar --applied

# 3. Continue with remaining migrations
snugom migrate deploy
```

#### Scenario 2: Need to Rollback

```bash
# 1. Manually revert your data/code changes

# 2. Mark the migration as rolled back
snugom migrate resolve 20241228_143000_add_avatar --rolled-back

# 3. The migration will be re-applied on next deploy
```

#### Scenario 3: Environment Out of Sync

```bash
# Check current state
snugom schema status

# If a migration shows as not applied but data is already migrated:
snugom migrate resolve 20241228_143000_add_avatar --applied
```

---

## Project Structure

After initialization, your project will have:

```
your-project/
├── .snugom/
│   ├── config.toml              # CLI configuration
│   └── schemas/                 # Schema snapshots
│       ├── User_v1_20241228_100000.json
│       ├── User_v2_20241228_143000.json
│       └── Guild_v1_20241228_100000.json
├── src/
│   ├── models/
│   │   ├── user.rs              # Your entity definitions
│   │   └── guild.rs
│   └── migrations/
│       ├── mod.rs               # Migration registry
│       ├── _20241228_100000_init.rs
│       └── _20241228_143000_add_avatar.rs
└── Cargo.toml
```

### File Naming Convention

Migration files use the format: `_YYYYMMDD_HHMMSS_<name>.rs`

- Leading underscore allows valid Rust module names starting with numbers
- Timestamp ensures chronological ordering
- Name describes the migration purpose

---

## Schema Snapshots

Schema snapshots are JSON files that capture your entity structure at a point in time. They enable accurate change detection.

### Snapshot Contents

```json
{
  "entity": "User",
  "schema": 2,
  "collection": "users",
  "source_file": "src/models/user.rs",
  "generated_at": "2024-12-28T14:30:00Z",
  "fields": [
    {
      "name": "user_id",
      "field_type": "String",
      "is_optional": false,
      "is_id": true
    },
    {
      "name": "email",
      "field_type": "String",
      "is_optional": false,
      "searchable": true,
      "filterable": "Tag"
    },
    {
      "name": "avatar_url",
      "field_type": "String",
      "is_optional": true
    }
  ],
  "relations": [],
  "unique_constraints": [
    {
      "fields": ["email"],
      "case_insensitive": false
    }
  ]
}
```

### Snapshot Management

- Snapshots are saved automatically when creating migrations
- They are stored in `.snugom/schemas/`
- Only the latest snapshot per entity is used for diffing
- Old snapshots can be kept for historical reference

---

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `REDIS_URL` | Yes* | Redis connection URL (required for `migrate deploy`, `schema status`, `schema validate`) |

*Not required for `init` or `migrate create` commands.

### Examples

```bash
# Local development
export REDIS_URL="redis://localhost:6379"

# With password
export REDIS_URL="redis://:password@localhost:6379"

# With database selection
export REDIS_URL="redis://localhost:6379/1"

# Production with TLS
export REDIS_URL="rediss://user:password@redis.example.com:6380"
```

---

## Global Options

These options are available for all commands:

| Option | Description |
|--------|-------------|
| `--output <format>` | Output format: `table` (default), `json` |
| `-q`, `--quiet` | Suppress output (only errors shown) |
| `-v`, `--verbose` | Enable verbose output |
| `--no-color` | Disable colored output |
| `--help` | Show help information |
| `--version` | Show version information |

**Examples:**

```bash
# Quiet mode for CI/CD
snugom migrate deploy --quiet

# Verbose output for debugging
snugom migrate create --name test --verbose

# JSON output for scripting
snugom schema status --output json

# Disable colors for logging
snugom migrate deploy --no-color
```

---

## Best Practices

### 1. Always Preview Before Deploying

```bash
# Check what will change
snugom schema diff

# Preview deployment
snugom migrate deploy --dry-run

# Then deploy
snugom migrate deploy
```

### 2. Use Meaningful Migration Names

```bash
# Good
snugom migrate create --name add_user_avatar
snugom migrate create --name split_user_name_fields
snugom migrate create --name add_guild_member_roles

# Bad
snugom migrate create --name update
snugom migrate create --name fix
snugom migrate create --name changes
```

### 3. Validate Before Adding Unique Constraints

```bash
# Always check for duplicates first
snugom schema validate users --field email
# Then add the constraint
```

### 4. Review STUB Migrations

When the CLI generates a STUB migration, it means automatic transformation isn't possible. Always review and implement the migration logic:

```rust
// Generated STUB migration - requires implementation
pub fn up(ctx: &mut MigrationContext) -> Result<()> {
    // TODO: Implement data transformation
    // Example: Convert user.age from String to i32
    for doc in ctx.scan_documents("users", Some(1))? {
        let age_str = doc.data["age"].as_str().unwrap_or("0");
        let age_int: i32 = age_str.parse().unwrap_or(0);
        // Update document with new type
    }
    Ok(())
}
```

### 5. Test Migrations on Staging First

```bash
# Deploy to staging
REDIS_URL="redis://staging:6379" snugom migrate deploy

# Verify
REDIS_URL="redis://staging:6379" snugom schema status

# Then production
REDIS_URL="redis://production:6379" snugom migrate deploy
```

### 6. Commit Migration Files

Always commit your migration files and snapshots to version control:

```bash
git add src/migrations/
git add .snugom/schemas/
git commit -m "Add migration: add_user_avatar"
```

---

## Troubleshooting

### "SnugOM is not initialized"

```
Error: SnugOM is not initialized in this project.
Run 'snugom init' first to initialize.
```

**Solution:** Run `snugom init` to initialize the project.

---

### "REDIS_URL not set"

```
Error: REDIS_URL environment variable not set
```

**Solution:** Set the `REDIS_URL` environment variable:
```bash
export REDIS_URL="redis://localhost:6379"
```

---

### "No SnugomEntity types found"

```
Warning: No SnugomEntity types found in project
```

**Possible causes:**
1. No structs have `#[derive(SnugomEntity)]`
2. The CLI is running from the wrong directory
3. Entity files are not in standard `src/` locations

**Solution:** Ensure your entities use the derive macro and run from the project root.

---

### "Migration already applied"

When trying to re-run a migration that's already recorded:

```bash
# Check current state
snugom schema status

# If you need to re-run, first mark as rolled back
snugom migrate resolve <migration_name> --rolled-back

# Then deploy again
snugom migrate deploy
```

---

### "Checksum mismatch"

If a migration file was modified after being applied:

1. **Don't modify applied migrations** - Create a new migration instead
2. If you must fix it, use `resolve`:
   ```bash
   snugom migrate resolve <migration_name> --rolled-back
   # Fix the migration file
   snugom migrate deploy
   ```

---

### Connection Errors

```
Error: Failed to connect to Redis
```

**Check:**
1. Redis is running: `redis-cli ping`
2. URL is correct: `echo $REDIS_URL`
3. Redis has JSON module: `redis-cli MODULE LIST`
4. Network/firewall allows connection

---

For additional help, run:

```bash
snugom --help
snugom <command> --help
```
