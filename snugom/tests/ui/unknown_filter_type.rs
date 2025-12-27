//! Compile-fail test: #[snugom(filterable(invalid))] with unknown filter type.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: unknown filter type
    #[snugom(filterable(invalid))]
    pub name: String,
}

fn main() {}
