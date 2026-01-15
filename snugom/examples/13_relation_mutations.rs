use snugom::examples::repo::ex11_relation_mutations;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ex11_relation_mutations::run().await
}
