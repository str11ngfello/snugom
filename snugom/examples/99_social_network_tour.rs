use snugom::examples::client::social_network::tour;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tour::run().await
}
