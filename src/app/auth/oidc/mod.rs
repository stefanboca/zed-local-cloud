mod callback;

use rsa::RsaPublicKey;
use topcoat::{
    Result,
    context::{Cx, app_context},
    cookie::{Cookies as _, SameSite, cookie_store, private_cookies},
};
use url::Url;

use crate::{
    config::Config,
    oidc::{Oidc, OidcFlow},
};

pub(crate) async fn begin_oidc_flow(
    cx: &Cx,
    native_app_port: u16,
    native_app_public_key: RsaPublicKey,
) -> Result<Url> {
    let config = app_context::<Config>(cx);
    let authorization = app_context::<Oidc>(cx).begin_authorization(&config.oidc.scopes);
    cookie_store::<OidcFlow, _>(
        private_cookies(cx)
            .default_http_only(true)
            .default_same_site(SameSite::Lax)
            .default_path("/"),
        "oidc-flow",
    )
    .set(OidcFlow {
        state: authorization.state,
        nonce: authorization.nonce,
        pkce_verifier: authorization.pkce_verifier,
        native_app_port,
        native_app_public_key,
    })
    .commit()?;

    Ok(authorization.url)
}
