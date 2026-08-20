use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use serde::Deserialize;
use url::Url;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_bind_address")]
    pub bind_address: SocketAddr,
    pub base_url: Url,
    #[serde(default = "default_database_url")]
    pub database_url: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    pub oidc: OidcConfig,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OidcConfig {
    pub issuer_url: String,
    pub client_id: String,
    #[serde(default = "default_oidc_scopes")]
    pub scopes: Vec<String>,
    #[serde(default = "default_admin_claim")]
    pub admin_claim: String,
    #[serde(default = "default_admin_value")]
    pub admin_value: String,
}

pub struct Secrets {
    pub(crate) internal_api_key: String,
    pub(crate) oidc_client_secret: String,
}

pub struct InternalApiKey(pub(crate) String);

impl Config {
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("reading configuration {}", path.display()))?;
        toml::from_str(&contents).context("decoding TOML configuration")
    }

    pub fn dev() -> Self {
        Self {
            bind_address: "127.0.0.1:3000".parse().unwrap(),
            base_url: Url::parse("http://localhost:8787").unwrap(),
            database_url: default_database_url_dev(),
            log_level: "zed_local_cloud=debug,tower_http=info,sqlx=warn".to_owned(),
            oidc: OidcConfig {
                issuer_url: "http://localhost:9091".to_owned(),
                client_id: "zed-local".to_owned(),
                scopes: default_oidc_scopes(),
                admin_claim: "groups".to_owned(),
                admin_value: "zed-admin".to_owned(),
            },
        }
    }
}

impl Secrets {
    pub fn from_systemd() -> anyhow::Result<Self> {
        Ok(Self {
            internal_api_key: read_credential("internal-api-key")?,
            oidc_client_secret: read_credential("oidc-client-secret")?,
        })
    }

    pub fn dev() -> Self {
        Self {
            internal_api_key: "development-development-development".to_owned(),
            oidc_client_secret: "development".to_owned(),
        }
    }
}

fn read_credential(name: &str) -> anyhow::Result<String> {
    let directory =
        std::env::var("CREDENTIALS_DIRECTORY").context("CREDENTIALS_DIRECTORY is required")?;
    let value = fs::read_to_string(PathBuf::from(directory).join(name))
        .with_context(|| format!("reading credential {name}"))?;
    Ok(value.trim().to_owned())
}

fn default_bind_address() -> SocketAddr {
    "127.0.0.1:8787".parse().unwrap()
}

fn default_database_url() -> String {
    "postgres://zed@%2Frun%2Fpostgresql/zed".to_owned()
}

fn default_database_url_dev() -> String {
    "postgres://development:development@127.0.0.1:5432/zed".to_owned()
}

fn default_log_level() -> String {
    "zed_local_cloud=info,tower_http=info,sqlx=warn".to_owned()
}

fn default_oidc_scopes() -> Vec<String> {
    vec![
        "profile".to_owned(),
        "email".to_owned(),
        "groups".to_owned(),
    ]
}

fn default_admin_claim() -> String {
    "groups".to_owned()
}

fn default_admin_value() -> String {
    "admin".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_toml_config_shape() {
        let config: Config = toml::from_str(
            r#"
            base_url = "https://zed.example.com"

            [oidc]
            issuer_url = "https://auth.example.com"
            client_id = "zed-local"
            scopes = ["profile", "email", "groups"]
            admin_claim = "groups"
            admin_value = "zed-admin"
        "#,
        )
        .expect("valid config");

        assert_eq!(config.bind_address, default_bind_address());
        assert_eq!(config.base_url.as_str(), "https://zed.example.com/");
        assert_eq!(config.oidc.client_id, "zed-local");
        assert_eq!(config.oidc.scopes, ["profile", "email", "groups"]);
    }

    #[test]
    fn defaults_admin_access_to_the_admin_group() {
        let config: Config = toml::from_str(
            r#"
            base_url = "https://zed.example.com"

            [oidc]
            issuer_url = "https://auth.example.com"
            client_id = "zed-local"
        "#,
        )
        .expect("valid config");

        assert_eq!(config.oidc.scopes, ["profile", "email", "groups"]);
        assert_eq!(config.oidc.admin_claim, "groups");
        assert_eq!(config.oidc.admin_value, "admin");
    }

    #[test]
    fn rejects_unknown_config_keys() {
        assert!(
            toml::from_str::<Config>(
                r#"
            base_url = "https://zed.example.com"
            unknown = true
            [oidc]
            issuer_url = "https://auth.example.com"
            client_id = "zed-local"
        "#
            )
            .is_err()
        );
    }
}
