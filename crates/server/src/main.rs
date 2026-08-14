use std::{error::Error, path::PathBuf, sync::Arc};

use clap::{Parser, Subcommand};
use patchouli_backend::{BackendConfig, BackendEngine};
use patchouli_provider_sqlite::SqliteProvider;
use patchouli_server::{LocalClient, LocalServer, ServerOptions};

#[derive(Debug, Parser)]
#[command(
    name = "patchouli",
    version,
    about = "Patchouli local daemon and control CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the local daemon in the foreground.
    Serve {
        #[arg(long)]
        endpoint: String,
        /// SQLite database file opened by this daemon.
        #[arg(long)]
        database: PathBuf,
        /// Backend policy configuration loaded by the engine.
        #[arg(long)]
        config: PathBuf,
        #[arg(long, default_value = "local")]
        node_id: String,
        #[arg(long, default_value = "local")]
        cluster_id: String,
    },
    /// Report daemon readiness and process identity.
    Status {
        #[arg(long)]
        endpoint: String,
    },
    /// Ask the daemon to shut down cleanly.
    Stop {
        #[arg(long)]
        endpoint: String,
    },
    /// Validate configuration without starting the daemon.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Validate one backend policy configuration file.
    Check { path: PathBuf },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    match Cli::parse().command {
        Command::Serve {
            endpoint,
            database,
            config,
            node_id,
            cluster_id,
        } => {
            let input = std::fs::read_to_string(&config)?;
            let config = BackendConfig::from_json(&input)?;
            let provider = Arc::new(SqliteProvider::open(&database).await?);
            let engine = Arc::new(BackendEngine::start(config, provider).await?);
            let server = LocalServer::bind(
                ServerOptions {
                    endpoint: endpoint.clone(),
                    node_id,
                    cluster_id,
                },
                engine,
            )
            .await?;
            eprintln!("Patchouli daemon listening on {endpoint}");
            server.run().await?;
        }
        Command::Status { endpoint } => {
            let mut client =
                LocalClient::connect(&endpoint, "patchouli-cli", env!("CARGO_PKG_VERSION")).await?;
            println!("{}", serde_json::to_string_pretty(&client.status().await?)?);
        }
        Command::Stop { endpoint } => {
            let mut client =
                LocalClient::connect(&endpoint, "patchouli-cli", env!("CARGO_PKG_VERSION")).await?;
            let result = client.shutdown().await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Command::Config {
            command: ConfigCommand::Check { path },
        } => {
            let input = std::fs::read_to_string(&path)?;
            BackendConfig::from_json(&input)?;
            println!("valid: {}", path.display());
        }
    }
    Ok(())
}
