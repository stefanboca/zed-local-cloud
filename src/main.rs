use std::{env, path::PathBuf};

use anyhow::{Context as _, bail};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use zed_local_cloud::{Config, is_topcoat_dev, routes, serve};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if is_topcoat_dev() {
        init_tracing("debug")?;
        topcoat::start(routes().build())
            .await
            .context("UI development server failed")
    } else {
        let config = Config::from_file(&config_path()?)?;
        init_tracing(&config.log_level)?;
        serve(config).await
    }
}

fn init_tracing(log_level: &str) -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_new(log_level).context("log_level is not a valid tracing filter")?)
        .with(tracing_subscriber::fmt::layer())
        .try_init()
        .context("failed to initialize tracing")
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
