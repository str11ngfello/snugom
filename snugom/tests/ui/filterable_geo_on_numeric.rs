//! Entry 70: `filterable(geo)` on numeric types should fail.
//! GEO type requires "lat,lon" string format, not numbers.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: Cannot use filterable(geo) on numeric type
    #[snugom(filterable(geo))]
    pub latitude: f64,
}

fn main() {}
