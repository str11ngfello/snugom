use snugom::examples::example05_timestamps;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    example05_timestamps::run().await
}
