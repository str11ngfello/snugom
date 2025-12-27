use snugom::examples::example11_run_macro_crud;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    example11_run_macro_crud::run().await
}
