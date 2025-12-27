//! Compile-fail test: No field marked with #[snugom(id)].

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    // Missing #[snugom(id)]
    pub id: String,

    pub name: String,
}

fn main() {}
