use std::{env, path::PathBuf};

use anyhow::{anyhow, bail};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use zed_local_cloud::{Config, Secrets, serve};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (config, secrets) = if cfg!(debug_assertions) && env::var_os("TOPCOAT_DEV_URL").is_some() {
        (Config::dev(), Secrets::dev())
    } else {
        (
            Config::from_file(&config_path()?)?,
            Secrets::from_systemd()?,
        )
    };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level));
    if env::var_os("JOURNAL_STREAM").is_some() {
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_journald::layer()?)
            .try_init()
            .map_err(|error| anyhow!("initializing journald tracing: {error}"))?;
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .compact()
            .try_init()
            .map_err(|error| anyhow!("initializing terminal tracing: {error}"))?;
    }
    serve(config, secrets).await
}

fn config_path() -> anyhow::Result<PathBuf> {
    let mut arguments = env::args_os().skip(1);
    let Some(flag) = arguments.next() else {
        bail!("usage: zed-local-cloud --config <path>");
    };
    if flag != "--config" {
        bail!(
            "unknown argument {}; usage: zed-local-cloud --config <path>",
            flag.to_string_lossy()
        );
    }
    let Some(path) = arguments.next() else {
        bail!("--config requires a path");
    };
    if arguments.next().is_some() {
        bail!("usage: zed-local-cloud --config <path>");
    }
    Ok(PathBuf::from(path))
}
