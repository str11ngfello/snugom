use snugom::examples::example12_run_macro_nested;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    example12_run_macro_nested::run().await
}
