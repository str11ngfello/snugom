//! Compile-fail test: #[snugom(filterable(geo))] on boolean field.
//! GEO type requires "lat,lon" string format.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: geo can only be used on String fields
    #[snugom(filterable(geo))]
    pub has_location: bool,
}

fn main() {}
