use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::support;
use crate::repository::Repo;
use crate::search::SearchQuery;
use crate::SnugomEntity;

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "examples", collection = "search_events")]
struct SearchEvent {
    #[snugom(id)]
    id: String,
    #[snugom(datetime, sortable, alias = "occurred_at")]
    occurred_at: chrono::DateTime<Utc>,
    #[snugom(filterable(tag))]
    category: String,
    #[snugom(searchable)]
    message: String,
}

/// Example 14 – `SearchQuery` helpers with search filters.
pub async fn run() -> Result<()> {
    let mut conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("search_manager");
    let repo: Repo<SearchEvent> = Repo::new(prefix.clone());

    repo.ensure_search_index(&mut conn).await?;

    // Seed events.
    for (category, message) in [("info", "Welcome message"), ("error", "Something broke")] {
        repo.create_with_conn(
            &mut conn,
            SearchEvent::validation_builder()
                .category(category.to_string())
                .message(message.to_string())
                .occurred_at(Utc::now()),
        )
        .await?;
    }

    let query = SearchQuery {
        page: Some(1),
        page_size: Some(5),
        sort_by: None,
        sort_order: None,
        q: None,
        filter: vec!["category:eq:info".to_string()],
    };
    let results = repo.search_with_query(&mut conn, query).await?;
    assert_eq!(results.total, 1, "only info events returned");
    assert_eq!(results.items[0].message, "Welcome message");

    // Drop the index afterward to keep Redis tidy.
    let _: () = redis::cmd("FT.DROPINDEX")
        .arg(format!("{prefix}:idx"))
        .arg("DD")
        .query_async(&mut conn)
        .await
        .unwrap_or(());

    Ok(())
}
