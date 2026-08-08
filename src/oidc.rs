use std::collections::HashMap;

use anyhow::Context as _;
use openidconnect::{
    AdditionalClaims, Client, ClientId, ClientSecret, EmptyExtraTokenFields, EndpointMaybeSet,
    EndpointNotSet, EndpointSet, IdTokenFields, IssuerUrl, RedirectUrl, StandardErrorResponse,
    StandardTokenResponse,
    core::{
        CoreAuthDisplay, CoreAuthPrompt, CoreErrorResponseType, CoreGenderClaim, CoreJsonWebKey,
        CoreJweContentEncryptionAlgorithm, CoreProviderMetadata, CoreRevocableToken,
        CoreRevocationErrorResponse, CoreTokenIntrospectionResponse, CoreTokenType,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::{ParseError, Url};

use crate::config::Config;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct OidcClaims(pub HashMap<String, Value>);

impl AdditionalClaims for OidcClaims {}

pub type OidcTokenResponse = StandardTokenResponse<
    IdTokenFields<
        OidcClaims,
        EmptyExtraTokenFields,
        CoreGenderClaim,
        CoreJweContentEncryptionAlgorithm,
        openidconnect::core::CoreJwsSigningAlgorithm,
    >,
    CoreTokenType,
>;

pub type OidcClient = Client<
    OidcClaims,
    CoreAuthDisplay,
    CoreGenderClaim,
    CoreJweContentEncryptionAlgorithm,
    CoreJsonWebKey,
    CoreAuthPrompt,
    StandardErrorResponse<CoreErrorResponseType>,
    OidcTokenResponse,
    CoreTokenIntrospectionResponse,
    CoreRevocableToken,
    CoreRevocationErrorResponse,
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

pub struct Oidc {
    pub client: OidcClient,
    pub http_client: openidconnect::reqwest::Client,
}

impl Oidc {
    pub async fn discover(config: &Config) -> anyhow::Result<Self> {
        let callback_url = oidc_callback_url(&config.base_url)?;
        let http_client = openidconnect::reqwest::ClientBuilder::new()
            .redirect(openidconnect::reqwest::redirect::Policy::none())
            .build()
            .context("failed to construct the OIDC HTTP client")?;
        let issuer = IssuerUrl::new(config.oidc_issuer_url.clone())
            .context("the OIDC issuer URL is invalid")?;
        let provider_metadata = CoreProviderMetadata::discover_async(issuer, &http_client)
            .await
            .context("failed to discover the OIDC provider")?;
        let client_secret = config
            .oidc_client_secret
            .as_ref()
            .map(|secret| ClientSecret::new(secret.clone()));
        let client = OidcClient::from_provider_metadata(
            provider_metadata,
            ClientId::new(config.oidc_client_id.clone()),
            client_secret,
        )
        .set_redirect_uri(
            RedirectUrl::new(callback_url.to_string())
                .context("the OIDC redirect URL is invalid")?,
        );

        Ok(Self {
            client,
            http_client,
        })
    }
}

fn oidc_callback_url(base_url: &str) -> anyhow::Result<Url> {
    let mut callback_url = match Url::parse(base_url) {
        Ok(url) if url.has_host() => url,
        Ok(_) | Err(ParseError::RelativeUrlWithoutBase) => {
            Url::parse(&format!("https://{base_url}")).context("the base URL is invalid")?
        }
        Err(error) => return Err(error).context("the base URL is invalid"),
    };
    callback_url
        .set_scheme("https")
        .map_err(|()| anyhow::anyhow!("the base URL cannot use the https scheme"))?;
    callback_url.set_path("/auth/oidc/callback");
    callback_url.set_query(None);
    callback_url.set_fragment(None);
    Ok(callback_url)
}

pub fn claim_has_value(claims: &OidcClaims, path: &str, expected: &str) -> bool {
    claim_value(claims, path).is_some_and(|value| match value {
        Value::String(value) => value == expected,
        Value::Array(values) => values.iter().any(|value| value.as_str() == Some(expected)),
        _ => false,
    })
}

pub fn claim_value<'a>(claims: &'a OidcClaims, path: &str) -> Option<&'a Value> {
    let mut segments = path.split('.');
    let first = segments.next()?;
    let mut value = claims.0.get(first)?;
    for segment in segments {
        value = value.as_object()?.get(segment)?;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs_an_https_redirect_url_from_the_base_url() {
        assert_eq!(
            oidc_callback_url("zed.example.com/old?query=1#fragment")
                .expect("valid base URL")
                .as_str(),
            "https://zed.example.com/auth/oidc/callback"
        );
        assert_eq!(
            oidc_callback_url("http://zed.example.com")
                .expect("valid base URL")
                .as_str(),
            "https://zed.example.com/auth/oidc/callback"
        );
        assert_eq!(
            oidc_callback_url("localhost:8443")
                .expect("valid base URL")
                .as_str(),
            "https://localhost:8443/auth/oidc/callback"
        );
    }

    #[test]
    fn reads_scalar_array_and_nested_admin_claims() {
        let claims = OidcClaims(HashMap::from([
            ("role".to_owned(), Value::String("admin".to_owned())),
            (
                "groups".to_owned(),
                serde_json::json!(["users", "zed-admin"]),
            ),
            (
                "realm_access".to_owned(),
                serde_json::json!({"roles": ["admin"]}),
            ),
        ]));

        assert!(claim_has_value(&claims, "role", "admin"));
        assert!(claim_has_value(&claims, "groups", "zed-admin"));
        assert!(claim_has_value(&claims, "realm_access.roles", "admin"));
        assert!(!claim_has_value(&claims, "groups", "admin"));
        assert!(claim_value(&claims, "groups").is_some());
        assert!(claim_value(&claims, "missing").is_none());
    }
}
