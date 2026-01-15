use snugom::examples::repo::ex04_many_to_many;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ex04_many_to_many::run().await
}
