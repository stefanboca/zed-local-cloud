use anyhow::Context as _;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{app::App, config::Config};

mod api;
mod app;
mod config;
mod proxy;
mod store;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "zed_auth=info,tower_http=info,sqlx=warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .try_init()
        .context("failed to initialize tracing")?;

    App::new(Config::from_env()?).await?.serve().await
}
