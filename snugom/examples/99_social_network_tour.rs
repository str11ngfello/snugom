use snugom::examples::example99_social_network;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    example99_social_network::run().await
}
