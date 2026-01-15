#[tokio::test]
async fn account_and_profile_belongs_to() {
    snugom::examples::repo::ex02_belongs_to::run()
        .await
        .expect("example should succeed");
}
