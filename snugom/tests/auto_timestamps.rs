#[tokio::test]
async fn timestamps_example() {
    snugom::examples::example05_timestamps::run()
        .await
        .expect("example should succeed");
}
