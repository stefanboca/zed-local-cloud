mod _api;
mod auth;
mod channel;
mod favicon;
mod native_app_signin;
mod native_app_signin_succeeded;

use anyhow::Context as _;
use sha2::{Digest as _, Sha512};
use topcoat::{
    Result,
    asset::{AssetBundle, RouterBuilderAssetExt},
    context::Cx,
    cookie::{Key, RouterBuilderCookieExt},
    router::{
        RouterBuilder, StatusCode, href, layout, page, response::IntoResponse, tower::TowerLayer,
    },
    view::{StaticStr, View, view},
};
use tower_http::{
    LatencyUnit,
    trace::{DefaultMakeSpan, DefaultOnFailure, DefaultOnResponse, TraceLayer},
};
use tracing::Level;

use crate::{
    assets::ZED_LOGO,
    config::{Config, InternalApiKey, Secrets},
    database::Database,
    oidc::Oidc,
    user_updates::UserUpdates,
};

topcoat::router::not_found!();

pub async fn serve(config: Config, secrets: Secrets) -> anyhow::Result<()> {
    let bind_address = config.bind_address;
    let database = Database::new(&config.database_url).await?;
    let user_updates = UserUpdates::connect(&config.database_url).await?;
    let oidc = Oidc::discover(&config, secrets.oidc_client_secret).await?;
    let cookie_key = cookie_key(&secrets.internal_api_key);
    let internal_api_key = InternalApiKey(secrets.internal_api_key);

    let router = routes()
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
    notify_systemd_ready().context("failed to notify systemd that the service is ready")?;
    tracing::info!(%bind_address, "Zed local cloud listening");
    topcoat::serve(listener, router)
        .await
        .context("HTTP server failed")
}

#[cfg(target_os = "linux")]
fn notify_systemd_ready() -> std::io::Result<()> {
    use std::{
        env,
        os::{
            linux::net::SocketAddrExt as _,
            unix::{ffi::OsStrExt as _, net::UnixDatagram},
        },
    };

    let Some(address) = env::var_os("NOTIFY_SOCKET") else {
        return Ok(());
    };
    let address = address.as_os_str().as_bytes();
    let socket = UnixDatagram::unbound()?;
    if let Some(abstract_name) = address.strip_prefix(b"@") {
        let address = std::os::unix::net::SocketAddr::from_abstract_name(abstract_name)?;
        socket.send_to_addr(b"READY=1", &address)?;
    } else {
        socket.send_to(
            b"READY=1",
            std::path::Path::new(std::ffi::OsStr::from_bytes(address)),
        )?;
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn notify_systemd_ready() -> std::io::Result<()> {
    Ok(())
}

fn cookie_key(internal_api_key: &str) -> Key {
    let mut key_material = Sha512::new();
    key_material.update(b"zed-local-cloud OIDC cookie key\0");
    key_material.update(internal_api_key.as_bytes());
    Key::derive_from(&key_material.finalize())
}

pub fn routes() -> RouterBuilder {
    topcoat::router::module_router!()
        .layer(TowerLayer::new(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(
                    DefaultOnResponse::new()
                        .level(Level::INFO)
                        .latency_unit(LatencyUnit::Millis),
                )
                .on_failure(
                    DefaultOnFailure::new()
                        .level(Level::ERROR)
                        .latency_unit(LatencyUnit::Millis),
                ),
        ))
        .assets(AssetBundle::load().expect("failed to load the Topcoat asset bundle"))
}

#[layout]
async fn root_layout(cx: &Cx, slot: Result) -> Result {
    let content = match slot {
        Ok(content) => content,
        Err(error) => {
            let error_message = format!("{error:#}");
            let status = error
                .into_response(cx)
                .expect("error.into_response() always returns Ok")
                .status();
            if status.is_server_error() {
                tracing::error!(error = error_message, %status, "request handler failed");
            }
            friendly_error_page(cx, status).await?
        }
    };
    view! {
        cx =>
        <!DOCTYPE html>
        <html lang="en" class="scheme-dark">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"Zed local cloud"</title>
                <link rel="icon" type="image/svg+xml" href=(ZED_LOGO)>
                <link rel="stylesheet" href=(topcoat::tailwind::stylesheet!())>
                topcoat::runtime::script()
                topcoat::dev::script()
            </head>
            <body
                class="grid min-h-screen place-items-center bg-zinc-900 p-6 font-sans text-zinc-50"
            >
                (content)
            </body>
        </html>
    }
}

async fn friendly_error_page(cx: &Cx, status: StatusCode) -> Result<View> {
    let message = match status {
        StatusCode::BAD_REQUEST => {
            "We could not complete this sign-in request. Return to Zed and try again."
        }
        StatusCode::UNAUTHORIZED => "Sign in and try again.",
        StatusCode::FORBIDDEN => "Your account is not allowed to perform that action.",
        StatusCode::NOT_FOUND => "The page could not be found. Check the address and try again.",
        _ => "Something went wrong. Please try again.",
    };
    view! {
        cx =>
        (status)
        <main
            class="w-full max-w-md rounded-xl border border-zinc-700 bg-zinc-800 p-8 shadow-xl"
        >
            <p class="text-sm font-semibold uppercase tracking-widest text-violet-400">
                "Zed local cloud"
            </p>
            <h1 class="mt-3 text-3xl font-semibold">
                (status.as_u16())
                if let Some(title) = status.canonical_reason() {
                    " — "
                    (StaticStr(title))
                }
            </h1>
            <p class="mt-4 leading-6 text-zinc-300">(StaticStr(message))</p>
            <a
                class="mt-6 inline-flex rounded-lg bg-violet-600 px-4 py-2 font-semibold text-white hover:bg-violet-700"
                href=(href!(index))
            >
                "Return home"
            </a>
        </main>
    }
}

#[page]
async fn index() -> Result {
    view! {
        <main class="w-full max-w-md rounded-xl border border-zinc-700 bg-zinc-800 p-8">
            <h1 class="text-2xl font-semibold">"Zed local cloud"</h1>
            <p class="mt-4 leading-6 text-zinc-300">
                "This service is ready for Zed sign-in requests."
            </p>
        </main>
    }
}
