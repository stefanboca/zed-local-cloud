mod users;

use anyhow::Context as _;
use sqlx::{PgPool, postgres::PgPoolOptions};

pub use users::{AuthenticatedLocalUser, LocalUser};

use crate::oidc::OidcProfile;

#[derive(Clone)]
pub struct Database(PgPool);

impl Database {
    pub async fn new(url: &str) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(url)
            .await
            .context("failed to connect to Postgres")?;
        let mut migrator = sqlx::migrate!();
        migrator.set_ignore_missing(true);
        migrator
            .run(&pool)
            .await
            .context("failed to run database migrations")?;
        Ok(Self(pool))
    }

    pub async fn sync_oidc_user(
        &self,
        profile: &OidcProfile,
        token: &str,
        admin: bool,
    ) -> sqlx::Result<i32> {
        users::sync_oidc_user(&self.0, profile, token, admin).await
    }

    pub async fn authenticated_user(
        &self,
        user_id: i32,
        token: &str,
    ) -> sqlx::Result<Option<AuthenticatedLocalUser>> {
        users::authenticated_user(&self.0, user_id, token).await
    }

    pub async fn cached_token_is_valid(&self, user_id: i32, token: &str) -> sqlx::Result<bool> {
        users::cached_token_is_valid(&self.0, user_id, token).await
    }

    pub async fn users_by_ids(&self, user_ids: &[i32]) -> sqlx::Result<Vec<LocalUser>> {
        users::users_by_ids(&self.0, user_ids).await
    }

    pub async fn user_by_github_login(&self, login: &str) -> sqlx::Result<Option<LocalUser>> {
        users::user_by_github_login(&self.0, login).await
    }

    pub async fn fuzzy_search_users(
        &self,
        query: &str,
        limit: i64,
    ) -> sqlx::Result<Vec<LocalUser>> {
        users::fuzzy_search_users(&self.0, query, limit).await
    }

    pub async fn fuzzy_search_channel_members(
        &self,
        channel_id: i32,
        query: &str,
        limit: i64,
    ) -> sqlx::Result<Vec<users::ChannelMemberUser>> {
        users::fuzzy_search_channel_members(&self.0, channel_id, query, limit).await
    }
}
