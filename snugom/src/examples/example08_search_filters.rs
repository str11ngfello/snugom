use anyhow::Result;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::examples::support;
use crate::repository::Repo;
use crate::search::{SearchQuery, SortOrder};
use crate::{SnugomEntity, bundle};

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct SearchableGuild {
    #[snugom(id)]
    id: String,
    #[snugom(searchable, sortable)]
    name: String,
    #[snugom(searchable)]
    description: String,
    #[snugom(filterable(tag))]
    visibility: String,
    #[snugom(filterable(numeric), sortable)]
    member_count: i64,
    #[snugom(datetime(epoch_millis), sortable, alias = "created_at")]
    created_at: chrono::DateTime<Utc>,
}

bundle! {
    service: "examples",
    entities: { SearchableGuild => "searchable_guilds" }
}

/// Example 08 – building RediSearch queries with filters and sorts.
pub async fn run() -> Result<()> {
    let mut conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("search_filters");
    let repo: Repo<SearchableGuild> = Repo::new(prefix.clone());

    repo.ensure_search_index(&mut conn).await?;

    let now = Utc::now();
    for (name, visibility, count, offset) in [
        ("Sparks", "public", 42, 0),
        ("Hidden Base", "private", 7, 1),
        ("Public Plaza", "public", 128, 2),
    ] {
        repo.create_with_conn(
            &mut conn,
            SearchableGuild::validation_builder()
                .name(name.to_string())
                .description(format!("{name} description"))
                .visibility(visibility.to_string())
                .member_count(count)
                .created_at(now - Duration::hours(offset)),
        )
        .await?;
    }

    let query = SearchQuery {
        page: Some(1),
        page_size: Some(10),
        sort_by: Some("member_count".to_string()),
        sort_order: Some(SortOrder::Desc),
        q: None,
        filter: vec![
            "visibility:eq:public".to_string(),
            "member_count:range:10,+inf".to_string(),
        ],
    };

    let results = repo.search_with_query(&mut conn, query).await?;
    assert_eq!(results.total, 2, "two public guilds match");
    assert_eq!(
        results.items[0].name, "Public Plaza",
        "results sorted by descending member_count"
    );

    // Drop the index and keys to avoid polluting other tests/examples.
    let _: () = redis::cmd("FT.DROPINDEX")
        .arg(format!("{prefix}:idx"))
        .arg("DD")
        .query_async(&mut conn)
        .await
        .unwrap_or(());

    Ok(())
}
