use snugom::examples::repo::ex06_validation_rules;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ex06_validation_rules::run().await
}
