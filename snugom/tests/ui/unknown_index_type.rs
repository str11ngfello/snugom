//! Compile-fail test: #[snugom(indexed(invalid))] with unknown index type.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(schema = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: unknown index type
    #[snugom(indexed(invalid))]
    pub name: String,
}

fn main() {}
