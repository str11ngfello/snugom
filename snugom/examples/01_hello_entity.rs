use snugom::examples::repo::ex01_hello_entity;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ex01_hello_entity::run().await
}
