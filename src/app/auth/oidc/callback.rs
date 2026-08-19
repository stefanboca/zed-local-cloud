use anyhow::Context as _;
use base64::{Engine as _, prelude::BASE64_URL_SAFE};
use rand::RngExt as _;
use rsa::{Oaep, RsaPublicKey};
use sha2::Sha256;
use topcoat::{
    Result,
    context::{Cx, app_context},
    cookie::{Cookies as _, SameSite, cookie_store, private_cookies},
    router::{
        HeaderValue, StatusCode,
        error::{bad_request, internal_server_error},
        header, page, query_params,
    },
    view::view,
};
use url::Url;

use crate::{
    config::Config,
    database::Database,
    oidc::{Oidc, OidcFlow},
};

#[query_params(error = bad_request)]
struct OidcCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[page]
async fn callback(cx: &Cx) -> Result {
    let config = app_context::<Config>(cx);
    let database = app_context::<Database>(cx);
    let oidc_client = app_context::<Oidc>(cx);
    let query = query_params::<OidcCallbackQuery>(cx)?;
    if let Some(error) = &query.error {
        let description = query.error_description.as_deref().unwrap_or(error);
        return Err(bad_request(format!("the OIDC provider rejected login: {description}")).into());
    }
    let code = query
        .code
        .as_ref()
        .ok_or_else(|| bad_request("the OIDC callback has no code"))?;
    let returned_state = query
        .state
        .as_ref()
        .ok_or_else(|| bad_request("the OIDC callback has no state"))?;
    let login = cookie_store::<OidcFlow, _>(
        private_cookies(cx)
            .default_http_only(true)
            .default_same_site(SameSite::Lax)
            .default_path("/"),
        "oidc-flow",
    )
    .parse()?
    .ok_or_else(|| bad_request("the OIDC login flow is missing or invalid"))?;
    if login.read().state != *returned_state {
        return Err(bad_request("the OIDC state is invalid").into());
    }
    let login_value = login.get();
    login.remove();
    let login = login_value;

    let login_result = oidc_client
        .exchange_code(
            code,
            &login,
            &config.oidc.admin_claim,
            &config.oidc.admin_value,
        )
        .await
        .map_err(|error| internal("the OIDC login failed", error))?;

    let local_access_token = random_access_token();
    let user_id = database
        .sync_oidc_user(
            &login_result.profile,
            &local_access_token,
            login_result.admin,
        )
        .await
        .map_err(|error| internal("failed to synchronize the OIDC user", error))?;
    let encrypted_access_token =
        encrypt_access_token(&login.native_app_public_key, local_access_token.as_bytes())
            .map_err(|error| internal("failed to encrypt the Zed access token", error))?;

    let mut callback = Url::parse(&format!("http://127.0.0.1:{}/", login.native_app_port))
        .map_err(|error| internal("failed to build the native app callback", error))?;
    callback
        .query_pairs_mut()
        .append_pair("user_id", &user_id.to_string())
        .append_pair("access_token", &encrypted_access_token);
    let location = HeaderValue::try_from(callback.as_str())?;
    view! {
        cx =>
        (StatusCode::TEMPORARY_REDIRECT)
        ((header::LOCATION, location))
    }
}

fn random_access_token() -> String {
    let mut bytes = [0_u8; 48];
    rand::rng().fill(&mut bytes);
    BASE64_URL_SAFE.encode(bytes)
}

fn encrypt_access_token(public_key: &RsaPublicKey, token: &[u8]) -> anyhow::Result<String> {
    let encrypted = public_key
        .encrypt(&mut rand::rng(), Oaep::<Sha256>::new(), token)
        .context("RSA-OAEP encryption failed")?;
    Ok(BASE64_URL_SAFE.encode(encrypted))
}

fn internal(context: &'static str, error: impl Into<anyhow::Error>) -> topcoat::Error {
    let error = error.into().context(context);
    tracing::error!(%error, "request failed");
    internal_server_error(error).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_access_tokens_have_384_bits_of_random_data() {
        let token = random_access_token();
        assert_eq!(
            BASE64_URL_SAFE.decode(token).expect("valid base64").len(),
            48
        );
    }
}
