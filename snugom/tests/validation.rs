#[tokio::test]
async fn validation_rules_example() {
    snugom::examples::example06_validation_rules::run()
        .await
        .expect("example should succeed");
}
