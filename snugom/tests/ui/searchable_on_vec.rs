//! Compile-fail test: #[snugom(searchable)] on Vec field.
//! Arrays cannot be full-text searched directly.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: searchable only works on String fields
    #[snugom(searchable)]
    pub tags: Vec<String>,
}

fn main() {}
