#[tokio::test]
async fn cascade_strategies_example() {
    snugom::examples::example09_cascade_strategies::run()
        .await
        .expect("example should succeed");
}
