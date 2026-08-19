use anyhow::Context as _;
use base64::{Engine as _, prelude::BASE64_URL_SAFE};
use rsa::{RsaPublicKey, pkcs1::DecodeRsaPublicKey};
use topcoat::{
    Result,
    context::Cx,
    router::{error::bad_request, page, query_params},
    view::view,
};

use super::auth;

topcoat::router::segment!(rename = "native_app_signin");

#[query_params(error = bad_request)]
struct NativeAppSignInQuery {
    native_app_port: u16,
    native_app_public_key: String,
    system_id: Option<String>,
}

#[page]
pub(crate) async fn sign_in(cx: &Cx) -> Result {
    let query = query_params::<NativeAppSignInQuery>(cx)?;
    let _ = &query.system_id;
    if query.native_app_port == 0 {
        return Err(bad_request("native_app_port must be greater than zero").into());
    }
    let native_app_public_key = decode_public_key(&query.native_app_public_key)
        .map_err(|error| bad_request(format!("native_app_public_key is invalid: {error}")))?;
    let authorization_url = auth::begin_oidc_flow(cx, query.native_app_port, native_app_public_key)
        .await?
        .to_string();

    view! {
        <main class="w-full max-w-lg">
            <h1 class="text-3xl italic">"Authenticating with OpenID Connect"</h1>
            <p class="mt-5 text-xl tracking-wide text-zinc-400">
                "You will automatically be redirected."
            </p>
        </main>
        $(raw!("window.location.href = ${authorization_url}", ""))
    }
}

fn decode_public_key(encoded: &str) -> anyhow::Result<RsaPublicKey> {
    let bytes = BASE64_URL_SAFE
        .decode(encoded)
        .context("key is not URL-safe base64")?;
    RsaPublicKey::from_pkcs1_der(&bytes).context("key is not a PKCS#1 RSA public key")
}
