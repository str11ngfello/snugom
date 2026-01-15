use snugom::examples::repo::ex10_idempotency;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ex10_idempotency::run().await
}
