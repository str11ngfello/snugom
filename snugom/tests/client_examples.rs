//! Integration tests that run all client examples.
//!
//! Each example has assertions that verify correct behavior.

// ============ CRUD Operations (01-06) ============

#[tokio::test]
async fn client_ex01_hello_client() {
    snugom::examples::client::ex01_hello_client::run()
        .await
        .expect("example should succeed");
}

#[tokio::test]
async fn client_ex02_create_operations() {
    snugom::examples::client::ex02_create_operations::run()
        .await
        .expect("example should succeed");
}

#[tokio::test]
async fn client_ex03_read_operations() {
    snugom::examples::client::ex03_read_operations::run()
        .await
        .expect("example should succeed");
}

#[tokio::test]
async fn client_ex04_update_operations() {
    snugom::examples::client::ex04_update_operations::run()
        .await
        .expect("example should succeed");
}

#[tokio::test]
async fn client_ex05_delete_operations() {
    snugom::examples::client::ex05_delete_operations::run()
        .await
        .expect("example should succeed");
}

#[tokio::test]
async fn client_ex06_upsert_operations() {
    snugom::examples::client::ex06_upsert_operations::run()
        .await
        .expect("example should succeed");
}

// ============ Search & Filtering (07-10) ============

#[tokio::test]
async fn client_ex07_search_basic() {
    snugom::examples::client::ex07_search_basic::run()
        .await
        .expect("example should succeed");
}

#[tokio::test]
async fn client_ex08_search_pagination() {
    snugom::examples::client::ex08_search_pagination::run()
        .await
        .expect("example should succeed");
}

#[tokio::test]
async fn client_ex09_search_advanced() {
    snugom::examples::client::ex09_search_advanced::run()
        .await
        .expect("example should succeed");
}

#[tokio::test]
async fn client_ex10_sorting_ordering() {
    snugom::examples::client::ex10_sorting_ordering::run()
        .await
        .expect("example should succeed");
}

// ============ Schema & Fields (11-15) ============

#[tokio::test]
async fn client_ex11_field_attributes() {
    snugom::examples::client::ex11_field_attributes::run()
        .await
        .expect("example should succeed");
}

#[tokio::test]
async fn client_ex12_timestamps() {
    snugom::examples::client::ex12_timestamps::run()
        .await
        .expect("example should succeed");
}

#[tokio::test]
async fn client_ex13_validation() {
    snugom::examples::client::ex13_validation::run()
        .await
        .expect("example should succeed");
}

#[tokio::test]
async fn client_ex14_unique_constraints() {
    snugom::examples::client::ex14_unique_constraints::run()
        .await
        .expect("example should succeed");
}

#[tokio::test]
async fn client_ex15_custom_ids() {
    snugom::examples::client::ex15_custom_ids::run()
        .await
        .expect("example should succeed");
}

// ============ Relations (16-18) ============

#[tokio::test]
async fn client_ex16_relations() {
    snugom::examples::client::ex16_relations::run()
        .await
        .expect("example should succeed");
}

#[tokio::test]
async fn client_ex17_relation_mutations() {
    snugom::examples::client::ex17_relation_mutations::run()
        .await
        .expect("example should succeed");
}

#[tokio::test]
async fn client_ex18_cascade_strategies() {
    snugom::examples::client::ex18_cascade_strategies::run()
        .await
        .expect("example should succeed");
}

// ============ Advanced Patterns (19-23) ============

#[tokio::test]
async fn client_ex19_multi_entity_client() {
    snugom::examples::client::ex19_multi_entity_client::run()
        .await
        .expect("example should succeed");
}

// TODO: These examples require rethinking how version fields work in SnugOM
#[tokio::test]
#[ignore = "version field handling needs rethink"]
async fn client_ex20_error_handling() {
    snugom::examples::client::ex20_error_handling::run()
        .await
        .expect("example should succeed");
}

#[tokio::test]
#[ignore = "version field handling needs rethink"]
async fn client_ex21_optimistic_locking() {
    snugom::examples::client::ex21_optimistic_locking::run()
        .await
        .expect("example should succeed");
}

#[tokio::test]
async fn client_ex22_idempotency_keys() {
    snugom::examples::client::ex22_idempotency_keys::run()
        .await
        .expect("example should succeed");
}

#[tokio::test]
async fn client_ex23_batch_workflows() {
    snugom::examples::client::ex23_batch_workflows::run()
        .await
        .expect("example should succeed");
}

// ============ Social Network Application ============

// TODO: Social network uses version fields which need rethink
#[tokio::test]
#[ignore = "version field handling needs rethink"]
async fn client_social_network_tour() {
    snugom::examples::client::social_network::tour::run()
        .await
        .expect("social network tour should succeed");
}
