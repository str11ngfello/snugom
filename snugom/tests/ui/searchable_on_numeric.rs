//! Entry 67: `searchable` on numeric types should fail.
//! Numbers cannot be full-text searched, use `filterable` instead.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: Cannot use searchable on numeric type
    #[snugom(searchable)]
    pub count: u32,
}

fn main() {}
