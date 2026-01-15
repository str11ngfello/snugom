#[tokio::test]
async fn timestamps_example() {
    snugom::examples::repo::ex05_timestamps::run()
        .await
        .expect("example should succeed");
}
