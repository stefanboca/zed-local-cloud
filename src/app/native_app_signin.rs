use anyhow::Context as _;
use base64::Engine as _;
use base64::prelude::BASE64_URL_SAFE;
use rsa::RsaPublicKey;
use rsa::pkcs1::DecodeRsaPublicKey;
use topcoat::Result;
use topcoat::context::Cx;
use topcoat::router::error::bad_request;
use topcoat::router::{page, query_params};
use topcoat::view::{View, view};

use super::auth;

topcoat::router::segment!(rename = "native_app_signin");

#[query_params(error = bad_request)]
struct NativeAppSignInQuery {
    native_app_port: u16,
    native_app_public_key: String,
    system_id: Option<String>,
}

#[page]
pub(crate) async fn sign_in(cx: &Cx) -> Result<impl View> {
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

    Ok(view! {
        <main class="w-full max-w-lg">
            <h1 class="text-3xl italic">"Authenticating with OpenID Connect"</h1>
            <p class="mt-5 text-xl tracking-wide text-zinc-400">
                "You will automatically be redirected."
            </p>
        </main>
        <a id="oidc-authorization" hidden="" href=(authorization_url)></a>
        <script>
            "window.location.href = document.getElementById('oidc-authorization').href;"
        </script>
    })
}

fn decode_public_key(encoded: &str) -> anyhow::Result<RsaPublicKey> {
    let bytes = BASE64_URL_SAFE
        .decode(encoded)
        .context("key is not URL-safe base64")?;
    RsaPublicKey::from_pkcs1_der(&bytes).context("key is not a PKCS#1 RSA public key")
}
