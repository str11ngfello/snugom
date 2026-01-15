use snugom::examples::repo::ex08_search_filters;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ex08_search_filters::run().await
}
