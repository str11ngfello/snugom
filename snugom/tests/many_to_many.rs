#[tokio::test]
async fn user_following_topics_many_to_many() {
    snugom::examples::repo::ex04_many_to_many::run()
        .await
        .expect("example should succeed");
}
