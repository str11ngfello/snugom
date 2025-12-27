//! Compile-fail test: #[snugom(searchable)] on DateTime field.
//! DateTimes cannot be full-text searched.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: searchable only works on String fields
    #[snugom(searchable)]
    pub created_at: DateTime<Utc>,
}

fn main() {}
