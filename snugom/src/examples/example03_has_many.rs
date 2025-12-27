use anyhow::Result;
use chrono::Utc;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use crate::examples::support;
use crate::{SnugomEntity, bundle, repository::Repo};

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct Blog {
    #[snugom(id)]
    id: String,
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    created_at: chrono::DateTime<Utc>,
    name: String,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct BlogPost {
    #[snugom(id)]
    id: String,
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    created_at: chrono::DateTime<Utc>,
    title: String,
    #[snugom(relation(cascade = "delete"))]
    blog_id: String,
}

bundle! {
    service: "examples",
    entities: {
        Blog => "blogs",
        BlogPost => "posts",
    }
}

/// Example 03 – has-many with nested relation connection and cascade delete.
pub async fn run() -> Result<()> {
    let mut conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("has_many");
    let blog_repo: Repo<Blog> = Repo::new(prefix.clone());
    let post_repo: Repo<BlogPost> = Repo::new(prefix);

    let blog = blog_repo
        .create_with_conn(&mut conn, Blog::validation_builder().name("Rust Notes".to_string()).created_at(Utc::now()))
        .await?;
    let blog_id = blog.id.clone();

    let post = post_repo
        .create_with_conn(
            &mut conn,
            BlogPost::validation_builder()
                .title("Hello HasMany".to_string())
                .created_at(Utc::now())
                .blog_id(blog_id.clone())
                .relation("blog", vec![blog_id.clone()], Vec::new()),
        )
        .await?;
    let post_id = post.id.clone();

    // With belongs_to relations, the link is established when creating the child entity.
    // The reverse index for cascade delete is automatically maintained.
    // Verify the post's relation to the blog is set
    let relation_key = post_repo.relation_key("blog", &post_id);
    let members: Vec<String> = conn.smembers(&relation_key).await?;
    assert_eq!(members, vec![blog_id.clone()]);

    // Deleting the blog should cascade to delete the post
    blog_repo.delete_with_conn(&mut conn, &blog_id, None).await?;
    let post_exists: bool = conn.exists(post_repo.entity_key(&post_id)).await?;
    assert!(!post_exists, "posts should be removed when parent blog cascades delete");
    Ok(())
}
