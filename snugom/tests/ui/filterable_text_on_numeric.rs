//! Entry 69: `filterable(text)` on numeric types should fail.
//! Numbers are always NUMERIC type, not TEXT.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: Cannot use filterable(text) on numeric type
    #[snugom(filterable(text))]
    pub count: u32,
}

fn main() {}
