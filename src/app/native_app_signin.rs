use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context as _;
use base64::{Engine as _, prelude::BASE64_URL_SAFE};
use rsa::{RsaPublicKey, pkcs1::DecodeRsaPublicKey};
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{
        error::{bad_request, internal_server_error, redirect},
        page, query_params,
    },
    session,
    view::view,
};

use crate::{app::AppState, store};

use super::{auth, is_topcoat_dev};

const LOGIN_LIFETIME: Duration = Duration::from_secs(10 * 60);

topcoat::router::segment!(rename = "native_app_signin");

#[query_params(error = bad_request)]
struct NativeAppSignInQuery {
    native_app_port: u16,
    native_app_public_key: String,
    system_id: Option<String>,
}

#[page]
async fn sign_in(cx: &Cx) -> Result {
    if !is_topcoat_dev() {
        let state = app_context::<AppState>(cx);
        let query = query_params::<NativeAppSignInQuery>(cx)?;
        let _ = &query.system_id;
        if query.native_app_port == 0 {
            return Err(bad_request("native_app_port must be greater than zero").into());
        }
        decode_public_key(&query.native_app_public_key)
            .map_err(|error| bad_request(format!("native_app_public_key is invalid: {error}")))?;

        if let Some(old_hash) = session::token_hash(cx).await? {
            store::delete_login_session(&state.database, &*old_hash)
                .await
                .map_err(|error| internal("failed to delete an old login session", error))?;

            if let Some(refreshed) = session::refresh(cx).await? {
                let authenticated_user_id = store::refresh_browser_session(
                    &state.database,
                    &*refreshed.token_hash,
                    refreshed.expires_at,
                )
                .await
                .map_err(|error| internal("failed to refresh the browser session", error))?;
                if authenticated_user_id.is_some() {
                    create_login_session(
                        state,
                        &*refreshed.token_hash,
                        query.native_app_port,
                        &query.native_app_public_key,
                    )
                    .await?;
                    let authorization_url = auth::authorization_url(cx).await?;
                    return Err(redirect(authorization_url.as_str()).into());
                }
            }
        }
        let login_session = session::start(cx).await?;
        create_login_session(
            state,
            &*login_session.token_hash,
            query.native_app_port,
            &query.native_app_public_key,
        )
        .await?;
    }

    view! {
        <main class="w-full max-w-md rounded-xl border border-zinc-700 bg-zinc-800 p-8">
            <h1 class="text-2xl font-semibold">"Sign in to Zed"</h1>
            <form method="post" action="/auth/oidc/start">
                <button
                    class="mt-4 w-full cursor-pointer rounded-lg bg-violet-600 px-4 py-3 font-semibold text-white hover:bg-violet-700"
                    type="submit"
                >
                    "Continue with OpenID Connect"
                </button>
            </form>
        </main>
    }
}

async fn create_login_session(
    state: &AppState,
    token_hash: &[u8],
    native_app_port: u16,
    native_app_public_key: &str,
) -> Result<()> {
    let expires_at = SystemTime::now()
        .checked_add(LOGIN_LIFETIME)
        .ok_or_else(|| {
            internal(
                "failed to calculate login session expiry",
                anyhow::anyhow!("system clock is out of range"),
            )
        })?;
    store::create_login_session(
        &state.database,
        token_hash,
        native_app_port,
        native_app_public_key,
        epoch_seconds(expires_at)?,
    )
    .await
    .map_err(|error| internal("failed to save the login session", error))
}

fn epoch_seconds(time: SystemTime) -> Result<i64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .ok_or_else(|| {
            internal(
                "failed to calculate session expiry",
                anyhow::anyhow!("system clock is out of range"),
            )
        })
}

fn decode_public_key(encoded: &str) -> anyhow::Result<RsaPublicKey> {
    let bytes = BASE64_URL_SAFE
        .decode(encoded)
        .context("key is not URL-safe base64")?;
    RsaPublicKey::from_pkcs1_der(&bytes).context("key is not a PKCS#1 RSA public key")
}

fn internal(context: &'static str, error: impl Into<anyhow::Error>) -> topcoat::Error {
    let error = error.into().context(context);
    tracing::error!(%error, "request failed");
    internal_server_error(error).into()
}
