//! Example 18 – Cascade Strategies
//!
//! Demonstrates cascade behavior when deleting related entities:
//! - `cascade = "delete"` - Delete related entities when parent is deleted
//! - No cascade - Related entities remain orphaned

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity, snugom_create, snugom_update, snugom_delete};

/// A folder that contains files. When deleted, files are cascaded.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "folders")]
struct Folder {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(text))]
    name: String,

    /// Files in this folder - CASCADE DELETE
    /// When the folder is deleted, all files are also deleted
    #[serde(default)]
    #[snugom(relation(target = "files", cascade = "delete"))]
    files: Vec<String>,
}

/// A file that belongs to a folder.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "files")]
struct File {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(text))]
    name: String,
    size: i64,
    #[snugom(relation(target = "folders"))]
    folder_id: String,
}

/// A project with tasks. No cascade - tasks remain when project deleted.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "projects")]
struct Project {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(text))]
    name: String,

    /// Tasks in this project - NO CASCADE
    /// Tasks remain when project is deleted (orphaned)
    #[serde(default)]
    #[snugom(relation(target = "cascade_tasks"))]
    tasks: Vec<String>,
}

/// A task that belongs to a project.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "cascade_tasks")]
struct Task {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(text))]
    title: String,
    #[snugom(relation(target = "projects"))]
    project_id: String,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [Folder, File, Project, Task])]
struct StorageClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("cascade");
    let mut client = StorageClient::new(conn, prefix);

    let mut folders = client.folders();
    let mut files = client.files();
    let mut projects = client.projects();
    let mut tasks = client.tasks();

    // ============ Cascade Delete Example ============
    {
        // Create a folder with files
        let folder_id = snugom_create!(client, Folder {
            name: "Documents".to_string(),
            created_at: Utc::now(),
        }).await?.id;

        let file1_id = snugom_create!(client, File {
            name: "report.pdf".to_string(),
            size: 1024,
            folder_id: folder_id.clone(),
            created_at: Utc::now(),
        }).await?.id;

        let file2_id = snugom_create!(client, File {
            name: "notes.txt".to_string(),
            size: 256,
            folder_id: folder_id.clone(),
            created_at: Utc::now(),
        }).await?.id;

        // Connect files to folder
        snugom_update!(client, Folder(entity_id = folder_id.clone()) {
            files: [
                connect file1_id.clone(),
                connect file2_id.clone(),
            ],
        }).await?;

        // Verify files exist
        assert!(files.exists(&file1_id).await?);
        assert!(files.exists(&file2_id).await?);

        // Delete the folder - files should be cascade deleted
        snugom_delete!(client, Folder(&folder_id)).await?;

        // Folder is gone
        assert!(!folders.exists(&folder_id).await?);

        // Files should also be gone due to cascade
        assert!(
            !files.exists(&file1_id).await?,
            "file1 should be cascade deleted"
        );
        assert!(
            !files.exists(&file2_id).await?,
            "file2 should be cascade deleted"
        );
    }

    // ============ No Cascade Example ============
    {
        // Create a project with tasks
        let project_id = snugom_create!(client, Project {
            name: "Website Redesign".to_string(),
            created_at: Utc::now(),
        }).await?.id;

        let task1_id = snugom_create!(client, Task {
            title: "Design mockups".to_string(),
            project_id: project_id.clone(),
            created_at: Utc::now(),
        }).await?.id;

        let task2_id = snugom_create!(client, Task {
            title: "Implement frontend".to_string(),
            project_id: project_id.clone(),
            created_at: Utc::now(),
        }).await?.id;

        // Connect tasks to project
        snugom_update!(client, Project(entity_id = project_id.clone()) {
            tasks: [
                connect task1_id.clone(),
                connect task2_id.clone(),
            ],
        }).await?;

        // Verify tasks exist
        assert!(tasks.exists(&task1_id).await?);
        assert!(tasks.exists(&task2_id).await?);

        // Delete the project - tasks should remain (orphaned)
        snugom_delete!(client, Project(&project_id)).await?;

        // Project is gone
        assert!(!projects.exists(&project_id).await?);

        // Tasks should still exist (no cascade)
        assert!(
            tasks.exists(&task1_id).await?,
            "task1 should remain (no cascade)"
        );
        assert!(
            tasks.exists(&task2_id).await?,
            "task2 should remain (no cascade)"
        );

        // Tasks now have an invalid project_id (orphaned)
        let orphan = tasks.get_or_error(&task1_id).await?;
        assert_eq!(orphan.project_id, project_id); // Still references deleted project
    }

    Ok(())
}
