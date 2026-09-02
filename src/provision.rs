use std::{
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
    livekit_key_file: Option<&Path>,
    livekit_secret_file: Option<&Path>,
) -> anyhow::Result<()> {
    let mut secrets = secretspec::Secrets::load()?.with_reason("provision Zed service secrets");
    secrets.set_profile("production");
    secrets.set_scope("generated");
    secrets.check(true)?;

    let secrets = config.secrets()?;

    // migrate the db and update pg_trgm
    Database::new(secrets.database_url.trim()).await?;

    let livekit = match (livekit_key_file, livekit_secret_file) {
        (Some(key), Some(secret)) => Some((
            fs::read_to_string(key).with_context(|| format!("reading {}", key.display()))?,
            fs::read_to_string(secret).with_context(|| format!("reading {}", secret.display()))?,
        )),
        (None, None) => None,
        _ => bail!("the LiveKit key and secret files must be provided together"),
    };
    let collab_secrets = CollabSecrets {
        database_url: secrets.database_url.trim(),
        internal_api_key: secrets.internal_api_key.trim(),
        livekit_key: livekit.as_ref().map(|(key, _)| key.trim()),
        livekit_secret: livekit.as_ref().map(|(_, secret)| secret.trim()),
    };
    write_atomic(
        collab_secret_file,
        toml::to_string(&collab_secrets).context("encoding collab secrets")?,
    )
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
