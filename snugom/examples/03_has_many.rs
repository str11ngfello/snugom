use snugom::examples::repo::ex03_has_many;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ex03_has_many::run().await
}
