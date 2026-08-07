use std::{env, path::PathBuf};

use anyhow::{Context as _, bail};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use zed_local_cloud::{App, Config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_file(&config_path()?)?;
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_new(&config.log_level)
                .context("log_level is not a valid tracing filter")?,
        )
        .with(tracing_subscriber::fmt::layer())
        .try_init()
        .context("failed to initialize tracing")?;

    App::new(config).await?.serve().await
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
