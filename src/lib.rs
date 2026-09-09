mod app;
mod assets;
mod config;
mod database;
mod oidc;
mod provision;
mod systemd_notify;
mod user_updates;

use anyhow::Context as _;
use base64::Engine as _;
use base64::prelude::BASE64_STANDARD;
pub use config::Config;
pub use provision::provision;
use topcoat::cookie::{Key, RouterBuilderCookieExt as _};

use crate::config::InternalApiKey;
use crate::database::Database;
use crate::oidc::Oidc;
use crate::user_updates::UserUpdates;

pub async fn serve(config: Config) -> anyhow::Result<()> {
    let secrets = config.secrets()?;

    let bind_address = config.bind_address;
    let database_url = secrets.database_url.trim();
    let database = Database::new(database_url).await?;
    let user_updates = UserUpdates::connect(database_url).await?;
    let oidc = Oidc::discover(&config, secrets.oidc_client_secret.trim().to_owned()).await?;

    let cookie_key = BASE64_STANDARD
        .decode(secrets.cookie_key.trim())
        .context("decoding COOKIE_KEY")?;
    let cookie_key = Key::try_from(cookie_key.as_slice())
        .context("COOKIE_KEY must decode to at least 64 bytes")?;
    let internal_api_key = InternalApiKey(secrets.internal_api_key.trim().to_owned());

    let router = app::routes()
        .cookies()
        .app_context(cookie_key)
        .app_context(config)
        .app_context(database)
        .app_context(internal_api_key)
        .app_context(oidc)
        .app_context(user_updates)
        .build();

    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .with_context(|| format!("failed to bind {bind_address}"))?;
    systemd_notify::notify_ready().context("failed to notify systemd that the service is ready")?;
    tracing::info!(%bind_address, "Zed local cloud listening");
    topcoat::serve(listener, router)
        .await
        .context("HTTP server failed")
}
