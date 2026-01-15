#[tokio::test]
async fn hello_entity_basic_crud() {
    snugom::examples::repo::ex01_hello_entity::run()
        .await
        .expect("example should succeed");
}

#[tokio::test]
async fn hello_entity_patch_update() {
    snugom::examples::repo::ex07_patch_updates::run()
        .await
        .expect("example should succeed");
}
