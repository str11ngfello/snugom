//! Compile-fail test: validate(each = "...") on non-collection field.
//! Each validation only works on Vec<T> fields.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: each validator not supported for string
    #[snugom(validate(each = "length(min = 1)"))]
    pub name: String,
}

fn main() {}
