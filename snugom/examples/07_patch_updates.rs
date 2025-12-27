use snugom::examples::example07_patch_updates;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    example07_patch_updates::run().await
}
