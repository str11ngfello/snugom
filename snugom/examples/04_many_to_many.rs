use snugom::examples::example04_many_to_many;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    example04_many_to_many::run().await
}
