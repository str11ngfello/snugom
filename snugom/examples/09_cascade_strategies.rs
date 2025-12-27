use snugom::examples::example09_cascade_strategies;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    example09_cascade_strategies::run().await
}
