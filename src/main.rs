use std::{env, path::PathBuf};

use anyhow::anyhow;
use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use zed_local_cloud::{Config, provision, serve};

#[derive(Parser)]
struct Arguments {
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Provision {
        #[arg(long, value_name = "PATH")]
        collab_secret_file: PathBuf,
        #[arg(long, value_name = "KEY")]
        livekit_api_key: Option<String>,
    },
    Serve,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let arguments = Arguments::parse();
    let command = arguments.command.unwrap_or(Command::Serve);
    match command {
        Command::Provision {
            collab_secret_file,
            livekit_api_key,
        } => {
            provision(
                Config::new(arguments.config.as_deref())?,
                &collab_secret_file,
                livekit_api_key.as_deref(),
            )
            .await?;
            return Ok(());
        }
        Command::Serve => {}
    }

    let config = Config::new(arguments.config.as_deref())?;
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
    serve(config).await
}
