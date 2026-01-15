//! Compile-fail test: validate(email) on numeric field.
//! Email validation only works on string fields.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(schema = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: email validator not supported for numeric
    #[snugom(validate(email))]
    pub contact: u64,
}

fn main() {}
