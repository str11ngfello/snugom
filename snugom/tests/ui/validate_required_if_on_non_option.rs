//! Compile-fail test: validate(required_if = "...") on non-Option field.
//! required_if validation only works on Option<T> fields.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    pub enabled: bool,

    // ERROR: required_if requires Option<T> field
    #[snugom(validate(required_if = "enabled"))]
    pub value: String,
}

fn main() {}
