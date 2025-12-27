//! Compile-fail test: #[snugom(datetime(epoch_millis))] on non-DateTime field.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: datetime(epoch_millis) requires DateTime type
    #[snugom(datetime(epoch_millis))]
    pub timestamp: i64,
}

fn main() {}
