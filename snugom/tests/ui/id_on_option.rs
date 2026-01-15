//! Compile-fail test: #[snugom(id)] on Option<String> field.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(schema = 1)]
pub struct InvalidEntity {
    // ERROR: id cannot be Optional
    #[snugom(id)]
    pub id: Option<String>,

    pub name: String,
}

fn main() {}
