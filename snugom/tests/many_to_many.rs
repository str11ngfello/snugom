#[tokio::test]
async fn user_following_topics_many_to_many() {
    snugom::examples::example04_many_to_many::run()
        .await
        .expect("example should succeed");
}
