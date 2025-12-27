#[tokio::test]
async fn search_filters_example() {
    snugom::examples::example08_search_filters::run()
        .await
        .expect("example should succeed");
}

#[tokio::test]
async fn search_manager_example() {
    snugom::examples::example14_search_manager::run()
        .await
        .expect("example should succeed");
}
