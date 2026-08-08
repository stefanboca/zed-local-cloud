mod _api;
mod auth;
mod native_app_signin;
mod native_app_signin_succeeded;

use std::{env, sync::Arc, time::Duration};

use anyhow::Context as _;
use sqlx::{PgPool, postgres::PgPoolOptions};
use topcoat::{
    Result,
    asset::{Asset, AssetBundle, RouterBuilderAssetExt},
    context::{Cx, app_context},
    cookie::RouterBuilderCookieExt,
    router::{RouterBuilder, error::not_found, layout, page},
    session::{RouterBuilderSessionExt, SessionConfig},
    tailwind,
    view::view,
};

use crate::{config::Config, oidc::Oidc};

const STYLESHEET: Asset = tailwind::stylesheet!();

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub database: PgPool,
    pub oidc: Arc<Oidc>,
}

pub async fn serve(config: Config) -> anyhow::Result<()> {
    let bind_address = config.bind_address;
    let browser_session_lifetime = Duration::from_secs(config.browser_session_lifetime_seconds);
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
    let state = AppState {
        config: Arc::new(config),
        database,
        oidc: Arc::new(oidc),
    };

    let router = routes()
        .cookies()
        .sessions(
            SessionConfig::builder()
                .lifetime(browser_session_lifetime)
                .build(),
        )
        .app_context(state)
        .build();

    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .with_context(|| format!("failed to bind {bind_address}"))?;
    tracing::info!(%bind_address, "Zed local cloud listening");
    topcoat::serve(listener, router)
        .await
        .context("HTTP server failed")
}

pub fn routes() -> RouterBuilder {
    topcoat::router::module_router!()
        .assets(AssetBundle::load().expect("failed to load the Topcoat asset bundle"))
}

pub fn is_topcoat_dev() -> bool {
    env::var_os("TOPCOAT_DEV_URL").is_some()
}

fn production_state(cx: &Cx) -> Result<&AppState> {
    if is_topcoat_dev() {
        Err(not_found().into())
    } else {
        Ok(app_context::<AppState>(cx))
    }
}

#[layout]
async fn root_layout(slot: Result) -> Result {
    view! {
        <!DOCTYPE html>
        <html lang="en" class="scheme-dark">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"Zed local cloud"</title>
                <link rel="stylesheet" href=(STYLESHEET)>
                topcoat::dev::script()
            </head>
            <body
                class="grid min-h-screen place-items-center bg-zinc-900 p-6 font-sans text-zinc-50"
            >
                (slot?)
            </body>
        </html>
    }
}

#[page]
async fn index() -> Result {
    if !is_topcoat_dev() {
        return Err(not_found().into());
    }

    view! {
        <main class="w-full max-w-md rounded-xl border border-zinc-700 bg-zinc-800 p-8">
            <h1 class="text-2xl font-semibold">"Zed local cloud UI previews"</h1>
            <ul class="mt-4 space-y-2">
                <li>
                    <a
                        class="text-violet-300 underline decoration-violet-500/50 underline-offset-4 hover:text-violet-200"
                        href="/native_app_signin"
                    >
                        "Sign in"
                    </a>
                </li>
                <li>
                    <a
                        class="text-violet-300 underline decoration-violet-500/50 underline-offset-4 hover:text-violet-200"
                        href="/native_app_signin_succeeded"
                    >
                        "Sign-in complete"
                    </a>
                </li>
            </ul>
        </main>
    }
}
