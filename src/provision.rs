use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write as _,
    os::unix::fs::OpenOptionsExt as _,
    path::Path,
};

use anyhow::{Context as _, bail};
use serde::Serialize;

use crate::{config::Config, database::Database};

#[derive(Serialize)]
struct CollabSecrets<'a> {
    #[serde(rename = "DATABASE_URL")]
    database_url: &'a str,
    #[serde(rename = "ZED_CLOUD_INTERNAL_API_KEY")]
    internal_api_key: &'a str,
    #[serde(rename = "LIVEKIT_KEY", skip_serializing_if = "Option::is_none")]
    livekit_key: Option<&'a str>,
    #[serde(rename = "LIVEKIT_SECRET", skip_serializing_if = "Option::is_none")]
    livekit_secret: Option<&'a str>,
}

pub async fn provision(
    config: Config,
    collab_secret_file: &Path,
    livekit_api_key: Option<&str>,
) -> anyhow::Result<()> {
    let mut secrets = secretspec::Secrets::load()?.with_reason("provision Zed service secrets");
    secrets.set_profile("production");
    secrets.set_scope("generated");
    secrets.check(true)?;

    let secrets = config.secrets()?;

    // migrate the db and update pg_trgm
    Database::new(secrets.database_url.trim()).await?;

    let livekit = match (secrets.livekit_keys.as_deref(), livekit_api_key) {
        (Some(keys), Some(api_key)) => Some((api_key, find_livekit_secret(keys, api_key)?)),
        (None, None) => None,
        (None, Some(_)) => bail!("LIVEKIT_KEYS is required when a LiveKit API key is configured"),
        (Some(_), None) => bail!("a LiveKit API key is required when LIVEKIT_KEYS is provided"),
    };
    let collab_secrets = CollabSecrets {
        database_url: secrets.database_url.trim(),
        internal_api_key: secrets.internal_api_key.trim(),
        livekit_key: livekit.as_ref().map(|(key, _)| *key),
        livekit_secret: livekit.as_ref().map(|(_, secret)| secret.as_str()),
    };
    write_atomic(
        collab_secret_file,
        toml::to_string(&collab_secrets).context("encoding collab secrets")?,
    )
}

fn find_livekit_secret(keys: &str, api_key: &str) -> anyhow::Result<String> {
    let keys: HashMap<String, String> =
        serde_yaml_ng::from_str(keys).context("decoding the LiveKit key file")?;
    let secret = keys
        .get(api_key)
        .with_context(|| format!("LiveKit API key {api_key:?} was not found"))?;
    if secret.is_empty() {
        bail!("LiveKit API key {api_key:?} has an empty secret");
    }
    Ok(secret.clone())
}

fn write_atomic(path: &Path, contents: String) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    let temporary = path.with_extension("tmp");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)
        .with_context(|| format!("creating {}", temporary.display()))?;
    file.write_all(contents.as_bytes())
        .with_context(|| format!("writing {}", temporary.display()))?;
    file.sync_all()
        .with_context(|| format!("syncing {}", temporary.display()))?;
    fs::rename(&temporary, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::find_livekit_secret;

    #[test]
    fn selects_a_livekit_application_from_a_shared_key_file() {
        let keys = "# shared LiveKit applications\nother: other-secret\n\"zed\": zed-secret:part\n";
        assert_eq!(find_livekit_secret(keys, "zed").unwrap(), "zed-secret:part");
    }

    #[test]
    fn rejects_missing_livekit_keys() {
        assert!(find_livekit_secret("other: secret\n", "zed").is_err());
    }
}
