use std::{collections::HashSet, env, net::SocketAddr, str::FromStr};

use anyhow::{Context as _, ensure};
use url::Url;

#[derive(Clone, Debug)]
pub struct Config {
    pub bind_address: SocketAddr,
    pub database_url: String,
    pub public_base_url: Url,
    pub local_collab_url: Url,
    pub upstream_web_url: Url,
    pub upstream_api_url: Url,
    pub upstream_cloud_url: Url,
    pub internal_api_key: String,
    pub admin_github_logins: HashSet<String>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let bind_address = parse("BIND_ADDRESS", env_or("BIND_ADDRESS", "127.0.0.1:8787"))?;
        let public_base_url = parse_url(
            "PUBLIC_BASE_URL",
            &env_or("PUBLIC_BASE_URL", "http://localhost:8787"),
        )?;
        let local_collab_url = parse_url(
            "LOCAL_COLLAB_URL",
            &env_or("LOCAL_COLLAB_URL", "http://127.0.0.1:8080"),
        )?;
        let upstream_web_url = parse_url(
            "UPSTREAM_WEB_URL",
            &env_or("UPSTREAM_WEB_URL", "https://zed.dev"),
        )?;
        let upstream_api_url = parse_url(
            "UPSTREAM_API_URL",
            &env_or("UPSTREAM_API_URL", "https://api.zed.dev"),
        )?;
        let upstream_cloud_url = parse_url(
            "UPSTREAM_CLOUD_URL",
            &env_or("UPSTREAM_CLOUD_URL", "https://cloud.zed.dev"),
        )?;
        let internal_api_key = env_or("ZED_CLOUD_INTERNAL_API_KEY", "internal-api-key-secret");
        ensure!(
            !internal_api_key.trim().is_empty(),
            "ZED_CLOUD_INTERNAL_API_KEY must not be empty"
        );

        let admin_github_logins = env::var("LOCAL_ADMIN_GITHUB_LOGINS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|login| !login.is_empty())
            .map(|login| login.to_lowercase())
            .collect();

        Ok(Self {
            bind_address,
            database_url: env_or("DATABASE_URL", "postgres://%2Frun%2Fpostgresql/zed"),
            public_base_url,
            local_collab_url,
            upstream_web_url,
            upstream_api_url,
            upstream_cloud_url,
            internal_api_key,
            admin_github_logins,
        })
    }

    pub fn is_admin(&self, github_login: &str) -> bool {
        self.admin_github_logins
            .contains(&github_login.to_lowercase())
    }

    pub fn public_url(&self, path: &str) -> Url {
        url_with_path(&self.public_base_url, path, None)
    }

    #[cfg(test)]
    pub fn test() -> Self {
        Self {
            bind_address: "127.0.0.1:8787".parse().expect("valid test address"),
            database_url: "postgres://localhost/zed".to_owned(),
            public_base_url: Url::parse("http://localhost:8787").expect("valid test URL"),
            local_collab_url: Url::parse("http://localhost:8080").expect("valid test URL"),
            upstream_web_url: Url::parse("https://zed.dev").expect("valid test URL"),
            upstream_api_url: Url::parse("https://api.zed.dev").expect("valid test URL"),
            upstream_cloud_url: Url::parse("https://cloud.zed.dev").expect("valid test URL"),
            internal_api_key: "test-internal-api-key".to_owned(),
            admin_github_logins: HashSet::new(),
        }
    }
}

pub fn url_with_path(base: &Url, path: &str, query: Option<&str>) -> Url {
    let mut url = base.clone();
    url.set_path(path);
    url.set_query(query);
    url
}

fn env_or(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_owned())
}

fn parse<T>(name: &str, value: String) -> anyhow::Result<T>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    value
        .parse()
        .with_context(|| format!("{name} has an invalid value"))
}

fn parse_url(name: &str, value: &str) -> anyhow::Result<Url> {
    let url = Url::parse(value).with_context(|| format!("{name} is not a valid URL"))?;
    ensure!(
        matches!(url.scheme(), "http" | "https"),
        "{name} must use http or https"
    );
    ensure!(url.host_str().is_some(), "{name} must include a host");
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacing_a_url_path_does_not_keep_the_base_path() {
        let base = Url::parse("https://example.com/base").expect("valid test URL");
        assert_eq!(
            url_with_path(&base, "/client/users/me", Some("a=1")).as_str(),
            "https://example.com/client/users/me?a=1"
        );
    }
}
