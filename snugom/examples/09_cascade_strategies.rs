use snugom::examples::repo::ex09_cascade_strategies;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ex09_cascade_strategies::run().await
}
