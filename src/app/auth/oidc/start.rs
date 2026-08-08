use openidconnect::{
    AuthenticationFlow, CsrfToken, Nonce, PkceCodeChallenge, Scope, core::CoreResponseType,
};
use sha2::{Digest as _, Sha256};
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{
        error::{SeeOther, bad_request, internal_server_error, see_other},
        route,
    },
    session,
};
use url::Url;

use crate::{app::AppState, store};

use super::super::super::is_topcoat_dev;

#[route(POST)]
async fn start(cx: &Cx) -> Result<SeeOther> {
    if is_topcoat_dev() {
        return Ok(see_other("/native_app_signin_succeeded"));
    }

    let authorization_url = authorization_url(cx).await?;
    Ok(see_other(authorization_url.as_str()))
}

pub(crate) async fn authorization_url(cx: &Cx) -> Result<Url> {
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

    Ok(authorization_url)
}

fn internal(context: &'static str, error: impl Into<anyhow::Error>) -> topcoat::Error {
    let error = error.into().context(context);
    tracing::error!(%error, "request failed");
    internal_server_error(error).into()
}
