//! Compile-fail test: #[snugom(searchable, filterable(tag))] on the same field.
//! These attributes are incompatible because searchable creates a TEXT index
//! (tokenized full-text search) while filterable(tag) expects a TAG index
//! (exact/prefix matching). TEXT indexes tokenize on punctuation, breaking
//! the exact matching semantics that TAG filters require.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: searchable and filterable(tag) cannot be combined
    #[snugom(searchable, filterable(tag))]
    pub name: String,
}

fn main() {}
