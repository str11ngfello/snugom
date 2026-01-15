use redis::Script;
use std::sync::LazyLock;

pub const ENTITY_MUTATION_SCRIPT_BODY: &str = include_str!("../../lua/entity_mutation.lua");
pub const ENTITY_PATCH_SCRIPT_BODY: &str = include_str!("../../lua/entity_patch.lua");
pub const ENTITY_DELETE_SCRIPT_BODY: &str = include_str!("../../lua/entity_delete.lua");
pub const ENTITY_UPSERT_SCRIPT_BODY: &str = include_str!("../../lua/entity_upsert.lua");
pub const ENTITY_GET_OR_CREATE_SCRIPT_BODY: &str = include_str!("../../lua/entity_get_or_create.lua");
pub const RELATION_MUTATION_SCRIPT_BODY: &str = include_str!("../../lua/relation_mutation.lua");

pub static ENTITY_MUTATION_SCRIPT: LazyLock<Script> = LazyLock::new(|| Script::new(ENTITY_MUTATION_SCRIPT_BODY));
pub static ENTITY_PATCH_SCRIPT: LazyLock<Script> = LazyLock::new(|| Script::new(ENTITY_PATCH_SCRIPT_BODY));
pub static ENTITY_DELETE_SCRIPT: LazyLock<Script> = LazyLock::new(|| Script::new(ENTITY_DELETE_SCRIPT_BODY));
pub static ENTITY_UPSERT_SCRIPT: LazyLock<Script> = LazyLock::new(|| Script::new(ENTITY_UPSERT_SCRIPT_BODY));
pub static ENTITY_GET_OR_CREATE_SCRIPT: LazyLock<Script> = LazyLock::new(|| Script::new(ENTITY_GET_OR_CREATE_SCRIPT_BODY));
pub static RELATION_MUTATION_SCRIPT: LazyLock<Script> = LazyLock::new(|| Script::new(RELATION_MUTATION_SCRIPT_BODY));
