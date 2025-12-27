use snugom::examples::example10_idempotency_versions;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    example10_idempotency_versions::run().await
}
