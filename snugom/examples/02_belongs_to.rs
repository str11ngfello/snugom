use snugom::examples::repo::ex02_belongs_to;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ex02_belongs_to::run().await
}
