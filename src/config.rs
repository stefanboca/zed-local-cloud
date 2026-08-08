use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, ensure};
use serde::Deserialize;
use url::Url;

const DEFAULT_BROWSER_SESSION_LIFETIME_SECONDS: u64 = 30 * 24 * 60 * 60;

#[derive(Clone, Debug)]
pub struct Config {
    pub bind_address: SocketAddr,
    pub base_url: String,
    pub browser_session_lifetime_seconds: u64,
    pub database_url: String,
    pub internal_api_key: String,
    pub log_level: String,
    pub oidc_issuer_url: String,
    pub oidc_client_id: String,
    pub oidc_client_secret: Option<String>,
    pub oidc_scopes: Vec<String>,
    pub oidc_admin_claim: String,
    pub oidc_admin_value: String,
}

impl Config {
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file {}", path.display()))?;
        let config: ConfigFile = toml::from_str(&contents)
            .with_context(|| format!("failed to parse config file {}", path.display()))?;
        config.load()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    #[serde(default = "default_bind_address")]
    bind_address: SocketAddr,
    base_url: String,
    #[serde(default = "default_browser_session_lifetime_seconds")]
    browser_session_lifetime_seconds: u64,
    #[serde(default = "default_database_url")]
    database_url: String,
    internal_api_key_file: PathBuf,
    #[serde(default = "default_log_level")]
    log_level: String,
    oidc: OidcConfigFile,
}

impl ConfigFile {
    fn load(self) -> anyhow::Result<Config> {
        ensure!(
            !self.base_url.trim().is_empty(),
            "base_url must not be empty"
        );
        ensure!(
            !self.database_url.trim().is_empty(),
            "database_url must not be empty"
        );
        ensure!(
            self.browser_session_lifetime_seconds > 0,
            "browser_session_lifetime_seconds must be greater than zero"
        );
        ensure!(
            !self.log_level.trim().is_empty(),
            "log_level must not be empty"
        );
        ensure!(
            !self.oidc.client_id.trim().is_empty(),
            "oidc.client_id must not be empty"
        );
        ensure!(
            !self.oidc.admin_claim.trim().is_empty(),
            "oidc.admin_claim must not be empty"
        );
        ensure!(
            !self.oidc.admin_value.trim().is_empty(),
            "oidc.admin_value must not be empty"
        );

        let internal_api_key = read_secret(&self.internal_api_key_file, "internal_api_key_file")?;
        let oidc_client_secret = self
            .oidc
            .client_secret_file
            .as_deref()
            .map(|path| read_secret(path, "oidc.client_secret_file"))
            .transpose()?;
        let oidc_scopes = self
            .oidc
            .scopes
            .into_iter()
            .map(|scope| scope.trim().to_owned())
            .filter(|scope| !scope.is_empty() && scope != "openid")
            .collect();

        Ok(Config {
            bind_address: self.bind_address,
            base_url: self.base_url,
            browser_session_lifetime_seconds: self.browser_session_lifetime_seconds,
            database_url: self.database_url,
            internal_api_key,
            log_level: self.log_level,
            oidc_issuer_url: checked_url("oidc.issuer_url", self.oidc.issuer_url)?,
            oidc_client_id: self.oidc.client_id,
            oidc_client_secret,
            oidc_scopes,
            oidc_admin_claim: self.oidc.admin_claim,
            oidc_admin_value: self.oidc.admin_value,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OidcConfigFile {
    issuer_url: String,
    client_id: String,
    client_secret_file: Option<PathBuf>,
    #[serde(default = "default_oidc_scopes")]
    scopes: Vec<String>,
    #[serde(default = "default_admin_claim")]
    admin_claim: String,
    #[serde(default = "default_admin_value")]
    admin_value: String,
}

fn read_secret(path: &Path, name: &str) -> anyhow::Result<String> {
    let value = fs::read_to_string(path)
        .with_context(|| format!("failed to read {name} at {}", path.display()))?;
    let value = value.trim().to_owned();
    ensure!(!value.is_empty(), "{name} must not be empty");
    Ok(value)
}

fn checked_url(name: &str, value: String) -> anyhow::Result<String> {
    parsed_url(name, value.clone())?;
    Ok(value)
}

fn parsed_url(name: &str, value: String) -> anyhow::Result<Url> {
    let url = Url::parse(&value).with_context(|| format!("{name} is not a valid URL"))?;
    ensure!(
        matches!(url.scheme(), "http" | "https"),
        "{name} must use http or https"
    );
    ensure!(url.host_str().is_some(), "{name} must include a host");
    Ok(url)
}

fn default_bind_address() -> SocketAddr {
    "127.0.0.1:8787"
        .parse()
        .expect("the default bind address is valid")
}

fn default_browser_session_lifetime_seconds() -> u64 {
    DEFAULT_BROWSER_SESSION_LIFETIME_SECONDS
}

fn default_database_url() -> String {
    "postgres://zed@%2Frun%2Fpostgresql/zed".to_owned()
}

fn default_log_level() -> String {
    "zed_local_cloud=info,sqlx=warn".to_owned()
}

fn default_oidc_scopes() -> Vec<String> {
    vec!["profile".to_owned(), "email".to_owned()]
}

fn default_admin_claim() -> String {
    "roles".to_owned()
}

fn default_admin_value() -> String {
    "admin".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_toml_config_shape() {
        let config: ConfigFile = toml::from_str(
            r#"
                base_url = "zed.example.com"
                internal_api_key_file = "/run/zed/internal-api-key"

                [oidc]
                issuer_url = "https://auth.example.com"
                client_id = "zed-local"
                client_secret_file = "/run/credentials/zed-local-cloud.service/oidc-client-secret"
                scopes = ["profile", "email", "groups"]
                admin_claim = "groups"
                admin_value = "zed-admin"
            "#,
        )
        .expect("valid config");

        assert_eq!(config.bind_address, default_bind_address());
        assert_eq!(config.base_url, "zed.example.com");
        assert_eq!(
            config.browser_session_lifetime_seconds,
            DEFAULT_BROWSER_SESSION_LIFETIME_SECONDS
        );
        assert_eq!(config.oidc.client_id, "zed-local");
        assert_eq!(config.oidc.scopes, ["profile", "email", "groups"]);
        assert_eq!(config.oidc.admin_claim, "groups");
    }

    #[test]
    fn rejects_unknown_config_keys() {
        let result = toml::from_str::<ConfigFile>(
            r#"
                base_url = "zed.example.com"
                internal_api_key_file = "/run/zed/internal-api-key"
                unknown = true

                [oidc]
                issuer_url = "https://auth.example.com"
                client_id = "zed-local"
            "#,
        );

        assert!(result.is_err());
    }

    #[test]
    fn rejects_a_zero_browser_session_lifetime() {
        let config: ConfigFile = toml::from_str(
            r#"
                base_url = "zed.example.com"
                browser_session_lifetime_seconds = 0
                internal_api_key_file = "/not-used"

                [oidc]
                issuer_url = "https://auth.example.com"
                client_id = "zed-local"
            "#,
        )
        .expect("valid TOML");

        let error = config.load().expect_err("zero lifetime must fail");
        assert!(
            error
                .to_string()
                .contains("browser_session_lifetime_seconds must be greater than zero")
        );
    }

    #[test]
    fn rejects_the_removed_oidc_redirect_url() {
        let result = toml::from_str::<ConfigFile>(
            r#"
                base_url = "zed.example.com"
                internal_api_key_file = "/run/zed/internal-api-key"

                [oidc]
                issuer_url = "https://auth.example.com"
                client_id = "zed-local"
                redirect_url = "https://other.example.com/auth/oidc/callback"
            "#,
        );

        assert!(result.is_err());
    }

    #[test]
    fn preserves_the_exact_oidc_issuer_identifier() {
        let issuer = "https://auth.example.com";
        assert_eq!(
            checked_url("oidc.issuer_url", issuer.to_owned()).expect("valid issuer"),
            issuer
        );
    }
}
