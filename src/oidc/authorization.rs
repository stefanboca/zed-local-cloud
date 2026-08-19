use std::ops::Deref;

use openidconnect::{
    AccessTokenHash, AuthenticationFlow, AuthorizationCode, CsrfToken, Nonce, OAuth2TokenResponse,
    PkceCodeChallenge, PkceCodeVerifier, Scope, TokenResponse as _,
    core::{CoreGenderClaim, CoreResponseType},
};
use rsa::RsaPublicKey;

use super::*;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct OidcFlow {
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
    pub native_app_port: u16,
    pub native_app_public_key: RsaPublicKey,
}

impl Oidc {
    pub fn begin_authorization(&self, scopes: &[String]) -> OidcAuthorization {
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let mut authorization = self.client.authorize_url(
            AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        );
        for scope in scopes {
            authorization = authorization.add_scope(Scope::new(scope.clone()));
        }
        let (url, state, nonce) = authorization.set_pkce_challenge(challenge).url();
        OidcAuthorization {
            url,
            state: state.secret().to_owned(),
            nonce: nonce.secret().to_owned(),
            pkce_verifier: verifier.secret().to_owned(),
        }
    }

    pub async fn exchange_code(
        &self,
        code: &str,
        flow: &OidcFlow,
        admin_claim: &str,
        admin_value: &str,
    ) -> anyhow::Result<OidcLogin> {
        let token_response = self
            .client
            .exchange_code(AuthorizationCode::new(code.to_owned()))
            .context("the OIDC token endpoint is not configured")?
            .set_pkce_verifier(PkceCodeVerifier::new(flow.pkce_verifier.clone()))
            .request_async(&self.http_client)
            .await
            .context("exchanging the OIDC authorization code")?;
        let id_token = token_response
            .id_token()
            .context("the OIDC provider returned no ID token")?;
        let verifier = self.client.id_token_verifier();
        let claims = id_token
            .claims(&verifier, &Nonce::new(flow.nonce.clone()))
            .context("validating the OIDC ID token")?;

        if let Some(expected_hash) = claims.access_token_hash() {
            let actual_hash = AccessTokenHash::from_token(
                token_response.access_token(),
                id_token.signing_alg()?,
                id_token.signing_key(&verifier)?,
            )?;
            anyhow::ensure!(
                &actual_hash == expected_hash,
                "the OIDC access-token hash did not match the ID token"
            );
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
        let mut admin = claim_has_value(claims.additional_claims(), admin_claim, admin_value);

        if let Ok(user_info_request) = self.client.user_info(
            token_response.access_token().clone(),
            Some(claims.subject().clone()),
        ) {
            let user_info = user_info_request
                .request_async::<OidcClaims, _, CoreGenderClaim>(&self.http_client)
                .await
                .context("requesting OIDC UserInfo")?;
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
            if claim_value(user_info.additional_claims(), admin_claim).is_some() {
                admin = claim_has_value(user_info.additional_claims(), admin_claim, admin_value);
            }
        }

        Ok(OidcLogin { profile, admin })
    }
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
