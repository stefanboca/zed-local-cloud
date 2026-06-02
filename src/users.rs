use axum_login::{AuthUser, AuthnBackend, UserId};
use openidconnect::{
    AccessTokenHash, AuthorizationCode, ClaimsVerificationError, ClientId, ClientSecret,
    ConfigurationError, CsrfToken, HttpClientError, IssuerUrl, Nonce, OAuth2TokenResponse,
    PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, RequestTokenError,
    SignatureVerificationError, SigningError, StandardErrorResponse, TokenResponse,
    core::{CoreAuthenticationFlow, CoreClient, CoreErrorResponseType, CoreProviderMetadata},
    reqwest::{self, Client},
    url::Url,
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

#[derive(Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: String,
}

// Here we've implemented `Debug` manually to avoid accidentally logging the
// access token.
impl std::fmt::Debug for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("User").field("id", &self.id).finish()
    }
}

impl AuthUser for User {
    type Id = String;

    fn id(&self) -> Self::Id {
        self.id.clone()
    }

    fn session_auth_hash(&self) -> &[u8] {
        self.id.as_bytes()
    }
}

#[derive(Debug, Deserialize)]
pub struct Credentials {
    pub code: String,
    pub old_state: CsrfToken,
    pub new_state: CsrfToken,
    pub pkce_verifier: PkceCodeVerifier,
    pub nonce: Nonce,
}

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("sqlx error: {0}")]
    Sqlx(sqlx::Error),

    #[error("openid connect configuration: {0}")]
    OpenIDConnectConfiguration(ConfigurationError),

    #[error("openid request token: {0}")]
    OpenIDRequestToken(
        RequestTokenError<
            HttpClientError<reqwest::Error>,
            StandardErrorResponse<CoreErrorResponseType>,
        >,
    ),

    #[error("openid invalid token: {0}")]
    OpenIDInvalidToken(ClaimsVerificationError),

    #[error("openid signature verification: {0}")]
    OpenIDSignatureVerification(SignatureVerificationError),

    #[error("openid signing: {0}")]
    OpenIDSigning(SigningError),
}

#[derive(Debug, Clone)]
pub struct Backend {
    db: PgPool,
    client_id: ClientId,
    client_secret: Option<ClientSecret>,
    issuer_url: IssuerUrl,
    redirect_url: RedirectUrl,
    http_client: Client,
}

impl Backend {
    pub fn new(
        db: PgPool,
        client_id: ClientId,
        client_secret: Option<ClientSecret>,
        issuer_url: IssuerUrl,
        redirect_url: RedirectUrl,
    ) -> Self {
        let http_client = reqwest::ClientBuilder::new()
            // .tls_backend_native(true)
            // Following redirects opens the client up to SSRF vulnerabilities.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("Client should build");

        Self {
            db,
            client_id,
            client_secret,
            issuer_url,
            redirect_url,
            http_client,
        }
    }

    pub async fn authorize_url(&self) -> (Url, CsrfToken, Nonce, PkceCodeVerifier) {
        let provider_metadata =
            CoreProviderMetadata::discover_async(self.issuer_url.clone(), &self.http_client)
                .await
                .expect("Unable to get the core provider metadata");

        let client = CoreClient::from_provider_metadata(
            provider_metadata,
            self.client_id.clone(),
            self.client_secret.clone(),
        )
        .set_redirect_uri(self.redirect_url.clone());

        let (pkce_challenge, pkce_verifer) = PkceCodeChallenge::new_random_sha256();

        let (auth_url, csrf_token, nonce) = client
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .set_pkce_challenge(pkce_challenge)
            .url();

        (auth_url, csrf_token, nonce, pkce_verifer)
    }
}

impl AuthnBackend for Backend {
    type User = User;
    type Credentials = Credentials;
    type Error = BackendError;

    async fn authenticate(
        &self,
        creds: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        // Ensure the CSRF state has not been tampered with.
        if creds.old_state.secret() != creds.new_state.secret() {
            return Ok(None);
        };

        let provider_metadata =
            CoreProviderMetadata::discover_async(self.issuer_url.clone(), &self.http_client)
                .await
                .expect("Unable to get the core provider metadata");

        let client = CoreClient::from_provider_metadata(
            provider_metadata,
            self.client_id.clone(),
            self.client_secret.clone(),
        )
        .set_redirect_uri(self.redirect_url.clone());

        tracing::debug!(?creds.pkce_verifier);
        let token_response = client
            .exchange_code(AuthorizationCode::new(creds.code))
            .map_err(BackendError::OpenIDConnectConfiguration)?
            // Set the PKCE code verifier.
            .set_pkce_verifier(creds.pkce_verifier)
            .request_async(&self.http_client)
            .await
            // .inspect_err(|e| {
            //     if let RequestTokenError::Parse(_, source) = e {
            //         tracing::debug!("parse err: {}", String::from_utf8_lossy(source));
            //     }
            // })
            .map_err(BackendError::OpenIDRequestToken)?;

        let id_token = token_response
            .id_token()
            .expect("The provider MUST return an ID token for a valid token response");
        let id_token_verifier = client.id_token_verifier();
        let claims = id_token
            .claims(&id_token_verifier, &creds.nonce)
            .map_err(BackendError::OpenIDInvalidToken)?;

        if let Some(expected_access_token_hash) = claims.access_token_hash() {
            let actual_access_token_hash = AccessTokenHash::from_token(
                token_response.access_token(),
                id_token
                    .signing_alg()
                    .map_err(BackendError::OpenIDSignatureVerification)?,
                id_token
                    .signing_key(&id_token_verifier)
                    .map_err(BackendError::OpenIDSignatureVerification)?,
            )
            .map_err(BackendError::OpenIDSigning)?;
            if actual_access_token_hash != *expected_access_token_hash {
                return Ok(None);
            }
        }

        // Retrieve the locally-unique identifier from the decoded ID token
        let unique_identifier = claims.subject().as_str();

        // Persist user in our database so we can use `get_user`.
        let user = sqlx::query_as(
            r#"
            insert into users (id)
            values ($1)
            on conflict(id) do update
            set id = excluded.id
            returning *
            "#,
        )
        .bind(unique_identifier)
        .fetch_one(&self.db)
        .await
        .map_err(Self::Error::Sqlx)?;

        Ok(Some(user))
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        sqlx::query_as("select * from users where id = $1")
            .bind(user_id)
            .fetch_optional(&self.db)
            .await
            .map_err(Self::Error::Sqlx)
    }
}

// We use a type alias for convenience.
//
// Note that we've supplied our concrete backend here.
pub type AuthSession = axum_login::AuthSession<Backend>;
