//! Compile-fail test: #[snugom(id)] on non-String field.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(schema = 1)]
pub struct InvalidEntity {
    // ERROR: id must be String type
    #[snugom(id)]
    pub id: u64,

    pub name: String,
}

fn main() {}
