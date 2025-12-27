use chrono::{Duration, Utc};
use redis::aio::ConnectionManager;
use snugom::{
    repository::{MutationPayloadBuilder, RelationPlan, Repo},
    runtime::{MutationExecutor, RedisExecutor},
};

use super::models::{Comment, Post, User};

pub struct SocialNetworkRepos {
    pub users: Repo<User>,
    pub posts: Repo<Post>,
    pub comments: Repo<Comment>,
}

impl SocialNetworkRepos {
    pub fn new(prefix: impl Into<String>) -> Self {
        let prefix = prefix.into();
        Self {
            users: Repo::new(prefix.clone()),
            posts: Repo::new(prefix.clone()),
            comments: Repo::new(prefix),
        }
    }

    pub async fn reset(&self, executor: &mut impl MutationExecutor) -> anyhow::Result<()> {
        let mut conn = connection_manager().await?;
        let _: () = redis::cmd("FLUSHDB").query_async(&mut conn).await?;
        drop(conn);
        self.seed_baseline(executor).await
    }

    pub async fn seed_baseline(&self, executor: &mut impl MutationExecutor) -> anyhow::Result<()> {
        let now = Utc::now();
        let builder = User::validation_builder()
            .id("user_alice".to_string())
            .display_name("Alice".to_string())
            .bio(Some("Explorer".to_string()))
            .created_at(now);
        self.users.create(executor, builder).await?;

        let builder = User::validation_builder()
            .id("user_bob".to_string())
            .display_name("Bob".to_string())
            .bio(Some("Thinker".to_string()))
            .created_at(now + Duration::seconds(5))
            .connect("followers", vec!["user_alice".to_string()]);
        self.users.create(executor, builder).await?;

        // Alice creates a post
        let post_builder = Post::validation_builder()
            .id("post_alice_1".to_string())
            .title("Hello World".to_string())
            .body("Starting the journey".to_string())
            .created_at(now + Duration::seconds(10))
            .relation("author", vec!["user_alice".to_string()], Vec::new())
            .connect("liked_by", vec!["user_bob".to_string()]);
        self.posts
            .create(executor, post_builder)
            .await?;

        // Bob comments on Alice's post
        let comment_builder = Comment::validation_builder()
            .id("comment_bob_1".to_string())
            .body("Great start!".to_string())
            .created_at(now + Duration::seconds(20))
            .relation("author", vec!["user_bob".to_string()], Vec::new())
            .relation("post", vec!["post_alice_1".to_string()], Vec::new());
        self.comments
            .create(executor, comment_builder)
            .await?;

        Ok(())
    }

    pub async fn add_comment(
        &self,
        executor: &mut impl MutationExecutor,
        post_id: &str,
        author_id: &str,
        body: impl Into<String>,
    ) -> anyhow::Result<()> {
        let builder = Comment::validation_builder()
            .body(body.into())
            .created_at(Utc::now())
            .relation("author", vec![author_id.to_string()], Vec::new())
            .relation("post", vec![post_id.to_string()], Vec::new());
        let builder = builder.id(format!("comment_{}_{}", author_id, post_id));
        self.comments.create(executor, builder).await?;
        Ok(())
    }

    pub async fn follow(
        &self,
        executor: &mut impl MutationExecutor,
        follower_id: &str,
        followee_id: &str,
    ) -> anyhow::Result<()> {
        let relation = RelationPlan::with_left(
            "followers",
            followee_id,
            vec![follower_id.to_string()],
            Vec::new(),
        );
        self.users
            .mutate_relations(executor, vec![relation])
            .await?;
        Ok(())
    }
}

pub async fn connection_manager() -> anyhow::Result<ConnectionManager> {
    let client = redis::Client::open("redis://127.0.0.1/")?;
    Ok(client.get_connection_manager().await?)
}

pub async fn example_usage() -> anyhow::Result<()> {
    let mut conn = connection_manager().await?;
    let repos = SocialNetworkRepos::new("snug");
    let mut executor = RedisExecutor::new(&mut conn);

    repos.reset(&mut executor).await?;
    repos.follow(&mut executor, "user_alice", "user_bob").await?;
    repos
        .add_comment(&mut executor, "post_alice_1", "user_alice", "Thanks!")
        .await?;

    Ok(())
}
