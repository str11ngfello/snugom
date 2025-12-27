//! Compile-fail test: validate(unknown_rule) with invalid validation rule.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: unknown validation rule
    #[snugom(validate(unknown_rule))]
    pub name: String,
}

fn main() {}
