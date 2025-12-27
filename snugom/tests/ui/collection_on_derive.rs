use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(collection = "tests", version = 1)]
struct TestEntity {
    #[snugom(id)]
    id: String,
}

fn main() {}
