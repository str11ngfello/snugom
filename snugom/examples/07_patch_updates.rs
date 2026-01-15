use snugom::examples::repo::ex07_patch_updates;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ex07_patch_updates::run().await
}
