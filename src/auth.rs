use std::{
    ops::Deref,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::Context as _;
use base64::{Engine as _, prelude::BASE64_URL_SAFE};
use openidconnect::{
    AccessTokenHash, AuthenticationFlow, AuthorizationCode, CsrfToken, Nonce, OAuth2TokenResponse,
    PkceCodeChallenge, PkceCodeVerifier, Scope, TokenResponse as _,
    core::{CoreGenderClaim, CoreResponseType},
};
use rand::RngExt as _;
use rsa::{Oaep, RsaPublicKey, pkcs1::DecodeRsaPublicKey};
use sha2::{Digest as _, Sha256};
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{
        error::{SeeOther, bad_request, internal_server_error, see_other},
        query_params, route,
    },
    session,
};
use url::Url;

use crate::{
    app::AppState,
    oidc::{self, OidcClaims},
    store::{self, OidcProfile},
    ui,
};

const LOGIN_LIFETIME: Duration = Duration::from_secs(10 * 60);

#[query_params(error = bad_request)]
struct NativeAppSignInQuery {
    native_app_port: u16,
    native_app_public_key: String,
    system_id: Option<String>,
}

#[query_params(error = bad_request)]
struct OidcCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[route(GET "/native_app_signin")]
async fn native_app_sign_in(cx: &Cx) -> Result {
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

    ui::sign_in(cx, false).await
}

#[route(POST "/auth/oidc/start")]
async fn start_oidc(cx: &Cx) -> Result<SeeOther> {
    let state = app_context::<AppState>(cx);
    let token_hash = session::token_hash(cx)
        .await?
        .ok_or_else(|| bad_request("the login session is missing"))?;
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let mut request = state.oidc.client.authorize_url(
        AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
        CsrfToken::new_random,
        Nonce::new_random,
    );
    for scope in &state.config.oidc_scopes {
        request = request.add_scope(Scope::new(scope.clone()));
    }
    let (authorization_url, csrf_state, nonce) = request.set_pkce_challenge(pkce_challenge).url();
    let stored = store::start_oidc_login(
        &state.database,
        &*token_hash,
        &Sha256::digest(csrf_state.secret().as_bytes()),
        nonce.secret(),
        pkce_verifier.secret(),
    )
    .await
    .map_err(|error| internal("failed to update the login session", error))?;
    if !stored {
        return Err(bad_request("the login session expired").into());
    }

    Ok(see_other(authorization_url.as_str()))
}

#[route(GET "/auth/oidc/callback")]
async fn oidc_callback(cx: &Cx) -> Result<SeeOther> {
    let state = app_context::<AppState>(cx);
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
    let token_hash = session::token_hash(cx)
        .await?
        .ok_or_else(|| bad_request("the login session is missing"))?;
    let login = store::consume_oidc_login(
        &state.database,
        &*token_hash,
        &Sha256::digest(returned_state.as_bytes()),
    )
    .await
    .map_err(|error| internal("failed to read the login session", error))?
    .ok_or_else(|| bad_request("the login session is invalid or expired"))?;

    let token_response = state
        .oidc
        .client
        .exchange_code(AuthorizationCode::new(code.clone()))
        .map_err(|error| internal("the OIDC token endpoint is not configured", error))?
        .set_pkce_verifier(PkceCodeVerifier::new(login.oidc_pkce_verifier))
        .request_async(&state.oidc.http_client)
        .await
        .map_err(|error| internal("the OIDC code exchange failed", error))?;
    let id_token = token_response
        .id_token()
        .ok_or_else(|| bad_request("the OIDC provider returned no ID token"))?;
    let verifier = state.oidc.client.id_token_verifier();
    let nonce = Nonce::new(login.oidc_nonce);
    let claims = id_token
        .claims(&verifier, &nonce)
        .map_err(|error| bad_request(format!("the OIDC ID token is invalid: {error}")))?;

    if let Some(expected_hash) = claims.access_token_hash() {
        let signing_algorithm = id_token.signing_alg().map_err(|error| {
            bad_request(format!("the OIDC ID token algorithm is invalid: {error}"))
        })?;
        let signing_key = id_token
            .signing_key(&verifier)
            .map_err(|error| bad_request(format!("the OIDC ID token key is invalid: {error}")))?;
        let actual_hash = AccessTokenHash::from_token(
            token_response.access_token(),
            signing_algorithm,
            signing_key,
        )
        .map_err(|error| bad_request(format!("failed to verify the OIDC access token: {error}")))?;
        if &actual_hash != expected_hash {
            return Err(bad_request("the OIDC access token hash is invalid").into());
        }
    }

    let mut profile = OidcProfile {
        issuer: claims.issuer().to_string(),
        subject: claims.subject().to_string(),
        preferred_username: claims
            .preferred_username()
            .map(|value| value.as_str().to_owned()),
        avatar_url: localized_value(claims.picture()).unwrap_or_default(),
        name: localized_value(claims.name()),
        email_address: claims.email().map(|value| value.as_str().to_owned()),
    };
    let mut admin = oidc::claim_has_value(
        claims.additional_claims(),
        &state.config.oidc_admin_claim,
        &state.config.oidc_admin_value,
    );

    if let Ok(user_info_request) = state.oidc.client.user_info(
        token_response.access_token().clone(),
        Some(claims.subject().clone()),
    ) {
        let user_info = user_info_request
            .request_async::<OidcClaims, _, CoreGenderClaim>(&state.oidc.http_client)
            .await
            .map_err(|error| internal("the OIDC UserInfo request failed", error))?;
        profile.preferred_username = user_info
            .preferred_username()
            .map(|value| value.as_str().to_owned())
            .or(profile.preferred_username);
        profile.avatar_url = localized_value(user_info.picture()).unwrap_or(profile.avatar_url);
        profile.name = localized_value(user_info.name()).or(profile.name);
        profile.email_address = user_info
            .email()
            .map(|value| value.as_str().to_owned())
            .or(profile.email_address);
        if oidc::claim_value(
            user_info.additional_claims(),
            &state.config.oidc_admin_claim,
        )
        .is_some()
        {
            admin = oidc::claim_has_value(
                user_info.additional_claims(),
                &state.config.oidc_admin_claim,
                &state.config.oidc_admin_value,
            );
        }
    }

    let local_access_token = random_access_token();
    let user_id = store::sync_oidc_user(&state.database, &profile, &local_access_token, admin)
        .await
        .map_err(|error| internal("failed to synchronize the OIDC user", error))?;
    let encrypted_access_token =
        encrypt_access_token(&login.native_app_public_key, local_access_token.as_bytes())
            .map_err(|error| internal("failed to encrypt the Zed access token", error))?;

    session::stop(cx).await?;
    let mut callback = Url::parse(&format!("http://127.0.0.1:{}/", login.native_app_port))
        .map_err(|error| internal("failed to build the native app callback", error))?;
    callback
        .query_pairs_mut()
        .append_pair("user_id", &user_id.to_string())
        .append_pair("access_token", &encrypted_access_token);
    Ok(see_other(callback.as_str()))
}

#[route(GET "/native_app_signin_succeeded")]
async fn native_app_sign_in_succeeded(cx: &Cx) -> Result {
    ui::sign_in_succeeded(cx, false).await
}

fn localized_value<T: Deref<Target = String>>(
    claim: Option<&openidconnect::LocalizedClaim<T>>,
) -> Option<String> {
    let claim = claim?;
    claim
        .get(None)
        .or_else(|| claim.iter().next().map(|(_, value)| value))
        .map(|value| value.as_str().to_owned())
}

fn random_access_token() -> String {
    let mut bytes = [0_u8; 48];
    rand::rng().fill(&mut bytes);
    BASE64_URL_SAFE.encode(bytes)
}

fn decode_public_key(encoded: &str) -> anyhow::Result<RsaPublicKey> {
    let bytes = BASE64_URL_SAFE
        .decode(encoded)
        .context("key is not URL-safe base64")?;
    RsaPublicKey::from_pkcs1_der(&bytes).context("key is not a PKCS#1 RSA public key")
}

fn encrypt_access_token(encoded_key: &str, token: &[u8]) -> anyhow::Result<String> {
    let public_key = decode_public_key(encoded_key)?;
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
