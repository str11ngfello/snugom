use anyhow::Result;
use chrono::Utc;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use crate::examples::support;
use crate::{SnugomEntity, bundle, repository::Repo};

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct Account {
    #[snugom(id)]
    id: String,
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    created_at: chrono::DateTime<Utc>,
    email: String,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct Profile {
    #[snugom(id)]
    id: String,
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    created_at: chrono::DateTime<Utc>,
    display_name: String,
    #[snugom(relation(cascade = "delete"))]
    account_id: String,
}

bundle! {
    service: "examples",
    entities: {
        Account => "accounts",
        Profile => "profiles",
    }
}

/// Example 02 – belongs-to relationships with cascade delete.
pub async fn run() -> Result<()> {
    let mut conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("belongs_to");
    let account_repo: Repo<Account> = Repo::new(prefix.clone());
    let profile_repo: Repo<Profile> = Repo::new(prefix);

    let account = account_repo
        .create_with_conn(&mut conn, Account::validation_builder().email("hello@example.com".to_string()).created_at(Utc::now()))
        .await?;
    let account_id = account.id.clone();

    let profile = profile_repo
        .create_with_conn(
            &mut conn,
            Profile::validation_builder()
                .display_name("Hello".to_string())
                .created_at(Utc::now())
                .account_id(account_id.clone())
                .relation("account", vec![account_id.clone()], Vec::new()),
        )
        .await?;
    let profile_id = profile.id.clone();

    // No need to manually mutate relations - the account_id field establishes the relationship

    let forward_key = profile_repo.relation_key("account", &profile_id);
    let members: Vec<String> = conn.smembers(&forward_key).await?;
    assert_eq!(members, vec![account_id.clone()]);

    account_repo.delete_with_conn(&mut conn, &account_id, None).await?;
    let profile_exists: bool = conn.exists(profile_repo.entity_key(&profile_id)).await?;
    assert!(!profile_exists, "profile should be deleted because cascade = \"delete\"");
    Ok(())
}
