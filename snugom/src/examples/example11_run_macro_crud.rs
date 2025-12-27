use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::examples::support;
use crate::{SnugomEntity, bundle, repository::Repo};

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct MacroMember {
    #[snugom(id)]
    id: String,
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    joined_at: chrono::DateTime<Utc>,
    handle: String,
}

bundle! {
    service: "examples",
    entities: { MacroMember => "macro_members" }
}

/// Example 11 – basic CRUD using the `run!` macro.
pub async fn run() -> Result<()> {
    let mut conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("run_macro_crud");
    let repo: Repo<MacroMember> = Repo::new(prefix);

    // Create a member using the run! macro; the macro hides builder construction and executor wiring.
    let create_result = crate::run! {
        &repo,
        &mut conn,
        create => MacroMember {
            joined_at: Utc::now(),
            handle: "@macrofan".to_string(),
        }
    }?;
    let member_id = create_result.id.clone();

    // Fetch the member via run! get.
    let fetched = crate::run! {
        &repo,
        &mut conn,
        get => MacroMember(entity_id = member_id.clone())
    }?
    .expect("member should exist after create");
    assert_eq!(fetched.handle, "@macrofan");

    // Update the handle; optional setters (with `?`) only apply when the Option is Some.
    let new_handle = Some("@snugfan".to_string());
    crate::run! {
        &repo,
        &mut conn,
        update => MacroMember(entity_id = member_id.clone()) {
            handle?: new_handle,
        }
    }?;

    let updated = crate::run! {
        &repo,
        &mut conn,
        get => MacroMember(entity_id = member_id.clone())
    }?
    .expect("member should still exist after update");
    assert_eq!(updated.handle, "@snugfan");

    // Delete the member via run! delete.
    crate::run! {
        &repo,
        &mut conn,
        delete => MacroMember(entity_id = member_id.clone())
    }?;

    let deleted = crate::run! {
        &repo,
        &mut conn,
        get => MacroMember(entity_id = member_id.clone())
    }?;
    assert!(deleted.is_none(), "member should be gone after delete");

    Ok(())
}
