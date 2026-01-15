//! Compile-fail test: #[snugom(searchable)] on boolean field.
//! Booleans cannot be full-text searched.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(schema = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: searchable only works on String fields
    #[snugom(searchable)]
    pub is_active: bool,
}

fn main() {}
