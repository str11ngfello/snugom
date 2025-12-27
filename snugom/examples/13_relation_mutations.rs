use snugom::examples::example13_relation_mutations;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    example13_relation_mutations::run().await
}
