//! Compile-fail test: validate(url) on numeric field.
//! URL validation only works on string fields.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: url validator not supported for numeric
    #[snugom(validate(url))]
    pub link: i32,
}

fn main() {}
