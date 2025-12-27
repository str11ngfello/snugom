//! Compile-fail test: #[snugom(created_at)] on non-DateTime field.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: created_at requires DateTime type
    #[snugom(created_at)]
    pub created_at: i64,
}

fn main() {}
