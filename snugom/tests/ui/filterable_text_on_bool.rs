//! Compile-fail test: #[snugom(filterable(text))] on boolean field.
//! TEXT type is for full-text search on strings.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: text can only be used on String fields
    #[snugom(filterable(text))]
    pub is_published: bool,
}

fn main() {}
