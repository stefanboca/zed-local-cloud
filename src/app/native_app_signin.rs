use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context as _;
use base64::{Engine as _, prelude::BASE64_URL_SAFE};
use rsa::{RsaPublicKey, pkcs1::DecodeRsaPublicKey};
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{
        error::{bad_request, internal_server_error},
        page, query_params,
    },
    session,
    view::view,
};

use crate::{app::AppState, store};

use super::is_topcoat_dev;

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
        }
        let login_session = session::start(cx).await?;
        let expires_at_epoch = SystemTime::now()
            .checked_add(LOGIN_LIFETIME)
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .and_then(|duration| i64::try_from(duration.as_secs()).ok())
            .ok_or_else(|| {
                internal(
                    "failed to calculate login session expiry",
                    anyhow::anyhow!("system clock is out of range"),
                )
            })?;
        store::create_login_session(
            &state.database,
            &*login_session.token_hash,
            query.native_app_port,
            &query.native_app_public_key,
            expires_at_epoch,
        )
        .await
        .map_err(|error| internal("failed to save the login session", error))?;
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
