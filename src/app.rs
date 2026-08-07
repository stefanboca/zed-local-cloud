use std::{sync::Arc, time::Duration};

use anyhow::Context as _;
use sqlx::{PgPool, postgres::PgPoolOptions};
use topcoat::{
    cookie::RouterBuilderCookieExt,
    router::{Router, RouterBuilderDiscoverExt},
    session::{RouterBuilderSessionExt, SessionConfig},
};

use crate::{config::Config, oidc::Oidc};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub database: PgPool,
    pub oidc: Arc<Oidc>,
}

pub struct App {
    state: AppState,
}

impl App {
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let database = PgPoolOptions::new()
            .max_connections(10)
            .connect(&config.database_url)
            .await
            .context("failed to connect to Postgres")?;
        let mut migrator = sqlx::migrate!();
        migrator.set_ignore_missing(true);
        migrator
            .run(&database)
            .await
            .context("failed to run database migrations")?;
        let oidc = Oidc::discover(&config).await?;

        Ok(Self {
            state: AppState {
                config: Arc::new(config),
                database,
                oidc: Arc::new(oidc),
            },
        })
    }

    pub fn router(state: AppState) -> Router {
        Router::builder()
            .discover()
            .cookies()
            .sessions(
                SessionConfig::builder()
                    .lifetime(Duration::from_secs(10 * 60))
                    .build(),
            )
            .app_context(state)
            .build()
    }

    pub async fn serve(self) -> anyhow::Result<()> {
        let bind_address = self.state.config.bind_address;
        let listener = tokio::net::TcpListener::bind(bind_address)
            .await
            .with_context(|| format!("failed to bind {bind_address}"))?;
        tracing::info!(%bind_address, "Zed local cloud listening");
        topcoat::serve(listener, Self::router(self.state))
            .await
            .context("HTTP server failed")
    }
}
