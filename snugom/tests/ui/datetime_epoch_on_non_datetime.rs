//! Compile-fail test: #[snugom(datetime)] on non-DateTime field.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(schema = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: datetime requires DateTime type
    #[snugom(datetime)]
    pub timestamp: i64,
}

fn main() {}
