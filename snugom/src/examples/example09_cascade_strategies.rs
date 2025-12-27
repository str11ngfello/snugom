use anyhow::Result;
use chrono::Utc;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use crate::examples::support;
use crate::{SnugomEntity, bundle, repository::Repo};

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct Project {
    #[snugom(id)]
    id: String,
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    created_at: chrono::DateTime<Utc>,
    name: String,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct Task {
    #[snugom(id)]
    id: String,
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    created_at: chrono::DateTime<Utc>,
    title: String,
    #[snugom(relation(cascade = "delete"))]
    project_id: String,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct SubTask {
    #[snugom(id)]
    id: String,
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    created_at: chrono::DateTime<Utc>,
    description: String,
    #[snugom(relation(cascade = "delete"))]
    task_id: String,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct DetachGroup {
    #[snugom(id)]
    id: String,
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    created_at: chrono::DateTime<Utc>,
    name: String,
    #[snugom(relation(many_to_many = "detach_users", cascade = "detach"))]
    members_ids: Vec<String>,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct DetachUser {
    #[snugom(id)]
    id: String,
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    created_at: chrono::DateTime<Utc>,
    nickname: String,
    #[snugom(relation(many_to_many = "detach_groups", cascade = "detach"))]
    groups_ids: Vec<String>,
}

bundle! {
    service: "examples",
    entities: {
        Project => "projects",
        Task => "tasks",
        SubTask => "subtasks",
        DetachGroup => "detach_groups",
        DetachUser => "detach_users",
    }
}

/// Example 09 – cascade delete vs detach strategies.
pub async fn run() -> Result<()> {
    let mut conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("cascade");
    let projects: Repo<Project> = Repo::new(prefix.clone());
    let tasks: Repo<Task> = Repo::new(prefix.clone());
    let subtasks: Repo<SubTask> = Repo::new(prefix.clone());
    let detach_groups: Repo<DetachGroup> = Repo::new(prefix.clone());
    let detach_users: Repo<DetachUser> = Repo::new(prefix);

    // --- Cascade delete ---
    let project = projects
        .create_with_conn(
            &mut conn,
            Project::validation_builder()
                .name("Snug Roadmap".to_string())
                .created_at(Utc::now()),
        )
        .await?;
    let project_id = project.id.clone();

    let task = tasks
        .create_with_conn(
            &mut conn,
            Task::validation_builder()
                .title("Write docs".to_string())
                .created_at(Utc::now())
                .project_id(project_id.clone())
                .relation("project", vec![project_id.clone()], Vec::new()),
        )
        .await?;
    let task_id = task.id.clone();

    let subtask = subtasks
        .create_with_conn(
            &mut conn,
            SubTask::validation_builder()
                .description("Populate examples".to_string())
                .created_at(Utc::now())
                .task_id(task_id.clone())
                .relation("task", vec![task_id.clone()], Vec::new()),
        )
        .await?;
    let subtask_id = subtask.id.clone();

    // With belongs_to relations, the link is established when creating the child entity.
    // The reverse index for cascade delete is automatically maintained.

    projects.delete_with_conn(&mut conn, &project_id, None).await?;
    assert!(
        !conn.exists(tasks.entity_key(&task_id)).await?,
        "task removed when parent project deleted"
    );
    assert!(
        !conn.exists(subtasks.entity_key(&subtask_id)).await?,
        "subtask removed when parent project deleted"
    );

    // --- Cascade detach ---
    let group = detach_groups
        .create_with_conn(
            &mut conn,
            DetachGroup::validation_builder()
                .name("Macro Fans".to_string())
                .created_at(Utc::now())
                .members_ids(Vec::new()),
        )
        .await?;
    let group_id = group.id.clone();

    let user = detach_users
        .create_with_conn(
            &mut conn,
            DetachUser::validation_builder()
                .nickname("fan-1".to_string())
                .created_at(Utc::now())
                .groups_ids(Vec::new()),
        )
        .await?;
    let user_id = user.id.clone();

    detach_groups
        .mutate_relations_with_conn(
            &mut conn,
            vec![crate::repository::RelationPlan::with_left(
                "members_ids",
                group_id.clone(),
                vec![user_id.clone()],
                Vec::new(),
            )],
        )
        .await?;

    detach_groups.delete_with_conn(&mut conn, &group_id, None).await?;
    let still_exists: bool = conn.exists(detach_users.entity_key(&user_id)).await?;
    assert!(still_exists, "user should remain because cascade = \"detach\"");
    let members_relation = detach_groups.relation_key("members_ids", &group_id);
    let relation_exists: bool = conn.exists(&members_relation).await.unwrap_or(false);
    assert!(!relation_exists, "group relation set should be removed");
    let reverse_relation = detach_users.relation_key("groups_ids", &user_id);
    let reverse_members: Vec<String> = conn.smembers(&reverse_relation).await.unwrap_or_default();
    assert!(reverse_members.is_empty(), "user should no longer reference the deleted group");

    Ok(())
}
