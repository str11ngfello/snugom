use anyhow::Result;
use chrono::Utc;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use crate::examples::support;
use crate::{SnugomEntity, bundle, UpsertResult, repository::Repo};

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct MacroGuild {
    #[snugom(id)]
    guild_id: String,
    name: String,
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    created_at: chrono::DateTime<Utc>,
    #[serde(default)]
    #[snugom(relation(target = "macro_guild_members", cascade = "delete"))]
    guild_members: Vec<String>,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct MacroGuildMember {
    #[snugom(id)]
    member_id: String,
    user_id: String,
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    joined_at: chrono::DateTime<Utc>,
    #[snugom(relation(target = "macro_guilds"))]
    guild_id: String,
}

bundle! {
    service: "examples",
    entities: {
        MacroGuild => "macro_guilds",
        MacroGuildMember => "macro_guild_members",
    }
}

/// Example 12 – nested create/update/upsert flows with the `run!` macro.
pub async fn run() -> Result<()> {
    let mut conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("run_macro_nested");
    let guild_repo: Repo<MacroGuild> = Repo::new(prefix.clone());
    let _member_repo: Repo<MacroGuildMember> = Repo::new(prefix);

    // Nested create: create a guild and automatically create the leader member.
    let created = crate::run! {
        &guild_repo,
        &mut conn,
        create => MacroGuild {
            name: "Macro Mages".to_string(),
            created_at: Utc::now(),
            guild_members: [
                create MacroGuildMember {
                    user_id: "user-1".to_string(),
                    joined_at: Utc::now(),
                }
            ],
        }
    }?;
    let guild_id = created.id.clone();

    let relation_key = guild_repo.relation_key("guild_members", &guild_id);
    let member_ids: Vec<String> = conn.smembers(&relation_key).await?;
    assert_eq!(member_ids.len(), 1, "leader created via nested create");

    // Nested update: add another member during an update.
    crate::run! {
        &guild_repo,
        &mut conn,
        update => MacroGuild(entity_id = guild_id.clone()) {
            guild_members: [
                create MacroGuildMember {
                    user_id: "user-2".to_string(),
                    joined_at: Utc::now(),
                }
            ],
        }
    }?;
    let member_ids: Vec<String> = conn.smembers(&relation_key).await?;
    assert_eq!(member_ids.len(), 2, "second member created during update");

    // Upsert: first attempt hits the update branch, second call creates a new guild.
    let upsert_result = crate::run! {
        &guild_repo,
        &mut conn,
        upsert => MacroGuild() {
            update: MacroGuild(entity_id = guild_id.clone()) {
                name: "Macro Mages Updated".to_string(),
            },
            create: MacroGuild {
                name: "Should Not Create".to_string(),
                created_at: Utc::now(),
            }
        }
    }?;
    match upsert_result {
        UpsertResult::Updated(_) => {}
        other => panic!("expected update branch, got {other:?}"),
    }

    let upsert_create = crate::run! {
        &guild_repo,
        &mut conn,
        upsert => MacroGuild() {
            update: MacroGuild(entity_id = "non-existent".to_string()) {
                name: "No-op".to_string(),
            },
            create: MacroGuild {
                name: "Macro Newcomers".to_string(),
                created_at: Utc::now(),
            }
        }
    }?;
    match upsert_create {
        UpsertResult::Created(result) => {
            let new_guild = crate::run! {
                &guild_repo,
                &mut conn,
                get => MacroGuild(entity_id = result.id.clone())
            }?
            .expect("new guild should exist");
            assert_eq!(new_guild.name, "Macro Newcomers");
        }
        other => panic!("expected create branch, got {other:?}"),
    }

    Ok(())
}
