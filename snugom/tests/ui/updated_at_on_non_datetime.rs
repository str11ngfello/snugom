//! Compile-fail test: #[snugom(updated_at)] on non-DateTime field.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: updated_at requires DateTime type
    #[snugom(updated_at)]
    pub updated_at: String,
}

fn main() {}
