use snugom::examples::repo::ex12_search_manager;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ex12_search_manager::run().await
}
