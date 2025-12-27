//! Entry 68: `searchable` on enum types should fail.
//! Enums are TAG type (exact match), not TEXT. Use `filterable` instead.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Active,
    Inactive,
}

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: Cannot use searchable on enum type
    #[snugom(searchable)]
    pub status: Status,
}

fn main() {}
