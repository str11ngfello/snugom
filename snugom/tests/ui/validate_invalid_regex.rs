//! Compile-fail test: validate(regex = "...") with invalid regex pattern.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: invalid regex pattern (unclosed group)
    #[snugom(validate(regex = "^(unclosed"))]
    pub code: String,
}

fn main() {}
