#[tokio::test]
async fn blog_with_posts_has_many() {
    snugom::examples::repo::ex03_has_many::run()
        .await
        .expect("example should succeed");
}
