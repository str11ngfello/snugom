use snugom::examples::repo::ex05_timestamps;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ex05_timestamps::run().await
}
