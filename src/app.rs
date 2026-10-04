mod _api;
mod auth;
mod channel;
mod native_app_signin;
mod native_app_signin_succeeded;

use topcoat::Result;
use topcoat::asset::{AssetBundle, RouterBuilderAssetExt};
use topcoat::context::Cx;
use topcoat::router::error::{RedirectError, RewriteError};
use topcoat::router::response::IntoResponse;
use topcoat::router::tower::TowerLayer;
use topcoat::router::{RouterBuilder, Slot, StatusCode, href, layout, page};
use topcoat::runtime::RouterBuilderRuntimeExt as _;
use topcoat::view::{StaticStr, View, error_boundary, view};
use tower_http::LatencyUnit;
use tower_http::trace::{DefaultMakeSpan, DefaultOnFailure, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::assets::ZED_LOGO;

topcoat::router::not_found!();

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
        .runtime()
}

#[layout]
async fn root_layout(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
    Ok(view! {
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
                error_boundary(fallback: |error| friendly_error_page(cx, error), (slot))
            </body>
        </html>
    })
}

fn friendly_error_page(cx: &Cx, error: topcoat::Error) -> Result<impl View> {
    if error.downcast_ref::<RedirectError>().is_some()
        || error.downcast_ref::<RewriteError>().is_some()
    {
        return Err(error);
    }
    let status = error
        .into_response(cx)
        .expect("error.into_response() always returns Ok")
        .status();
    let message = match status {
        StatusCode::UNAUTHORIZED => "Sign in and try again.",
        StatusCode::FORBIDDEN => "Your account is not allowed to perform that action.",
        StatusCode::NOT_FOUND => "The page could not be found. Check the address and try again.",
        _ => "Something went wrong. Please try again.",
    };
    Ok(view! {
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
    })
}

#[page]
async fn index() -> Result<impl View> {
    Ok(view! {
        <main class="w-full max-w-md rounded-xl border border-zinc-700 bg-zinc-800 p-8">
            <h1 class="text-2xl font-semibold">"Zed local cloud"</h1>
        </main>
    })
}
