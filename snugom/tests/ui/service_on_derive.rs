use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(service = "test", version = 1)]
struct TestEntity {
    #[snugom(id)]
    id: String,
}

fn main() {}
