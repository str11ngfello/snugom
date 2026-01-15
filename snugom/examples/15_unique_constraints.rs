use snugom::examples::repo::ex13_unique_constraints;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ex13_unique_constraints::run().await
}
