use snugom::examples::example06_validation_rules;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    example06_validation_rules::run().await
}
