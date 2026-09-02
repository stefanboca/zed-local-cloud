mod users;

use anyhow::Context as _;
use sqlx::{PgPool, postgres::PgPoolOptions};

pub use users::{AuthenticatedLocalUser, LocalUser};

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
        sqlx::query_file!("queries/database/update_pg_trgm.sql")
            .execute(&pool)
            .await
            .context("failed to update pg_trgm")?;
        Ok(Self(pool))
    }
}
