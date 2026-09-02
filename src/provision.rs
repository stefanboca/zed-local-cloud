use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write as _,
    os::unix::fs::OpenOptionsExt as _,
    path::Path,
};

use anyhow::{Context as _, bail};

use crate::database::Database;

pub async fn provision(
    state_directory: &Path,
    livekit_api_key: Option<&str>,
) -> anyhow::Result<()> {
    let mut generated =
        secretspec::Secrets::load()?.with_reason("provision generated Zed service secrets");
    generated.set_profile("production");
    generated.set_scope("generated");
    generated.set_provider(format!("file:{}", state_directory.display()));
    generated.check(true)?;

    let mut supplied =
        secretspec::Secrets::load()?.with_reason("read administrator-supplied Zed secrets");
    supplied.set_profile("production");
    supplied.set_scope("provision");
    let supplied = supplied.resolve()?;
    if !supplied.is_ok() {
        bail!(
            "missing required provisioning secrets: {}",
            supplied.missing_required.join(", ")
        );
    }
    let database_url = supplied
        .secrets
        .get("DATABASE_URL")
        .and_then(|secret| secret.value.as_deref())
        .context("DATABASE_URL did not resolve to a value")?
        .trim();

    // migrate the db and update pg_trgm
    Database::new(database_url).await?;

    let livekit_keys = supplied
        .secrets
        .get("LIVEKIT_KEYS")
        .and_then(|secret| secret.value.as_deref());
    match (livekit_keys, livekit_api_key) {
        (Some(keys), Some(api_key)) => {
            write_atomic(&state_directory.join("LIVEKIT_KEY"), api_key)?;
            write_atomic(
                &state_directory.join("LIVEKIT_SECRET"),
                &find_livekit_secret(keys, api_key)?,
            )?;
        }
        (None, None) => {}
        (None, Some(_)) => bail!("LIVEKIT_KEYS is required when a LiveKit API key is configured"),
        (Some(_), None) => bail!("a LiveKit API key is required when LIVEKIT_KEYS is provided"),
    };
    Ok(())
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

fn write_atomic(path: &Path, contents: &str) -> anyhow::Result<()> {
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
