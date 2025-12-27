//! Compile-fail test: Multiple fields marked with #[snugom(id)].

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: Cannot have multiple id fields
    #[snugom(id)]
    pub another_id: String,

    pub name: String,
}

fn main() {}
