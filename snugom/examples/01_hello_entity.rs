use snugom::examples::example01_hello_entity;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    example01_hello_entity::run().await
}
