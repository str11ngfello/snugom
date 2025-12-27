use snugom::examples::example14_search_manager;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    example14_search_manager::run().await
}
