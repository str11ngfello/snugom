//! Compile-fail test: #[snugom(indexed)] on String without explicit type.
//! String fields require indexed(tag) or indexed(text).

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(schema = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: String indexed requires explicit type
    #[snugom(indexed)]
    pub name: String,
}

fn main() {}
