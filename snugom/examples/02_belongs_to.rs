use snugom::examples::example02_belongs_to;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    example02_belongs_to::run().await
}
