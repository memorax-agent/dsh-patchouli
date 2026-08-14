use std::{error::Error, net::SocketAddr, path::PathBuf, sync::Arc};

use clap::{Parser, Subcommand};
use patchouli_backend::{BackendConfig, BackendEngine};
use patchouli_provider::Provider;
use patchouli_provider_remote::remote_provider_router;
use patchouli_provider_sqlite::SqliteProvider;
use patchouli_server::{LocalClient, LocalServer, ServerOptions, load_provider};

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
    /// Run the backend daemon in the foreground.
    Serve {
        #[arg(long)]
        endpoint: String,
        /// Physical provider and scope-routing configuration.
        #[arg(long)]
        providers: PathBuf,
        /// Backend policy configuration loaded by the engine.
        #[arg(long)]
        config: PathBuf,
        #[arg(long, default_value = "local")]
        node_id: String,
        #[arg(long, default_value = "local")]
        cluster_id: String,
    },
    /// Serve one SQLite authority through the authenticated remote-provider protocol.
    Provide {
        #[arg(long)]
        listen: SocketAddr,
        #[arg(long)]
        database: PathBuf,
        /// Environment variable containing the bearer token.
        #[arg(long)]
        token_env: String,
    },
    /// Report daemon readiness and process identity.
    Status {
        #[arg(long)]
        endpoint: String,
    },
    /// Checkpoint durable provider state without stopping the daemon.
    Checkpoint {
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
    Check {
        path: PathBuf,
        /// Also validate a physical provider/routing configuration.
        #[arg(long)]
        providers: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    match Cli::parse().command {
        Command::Serve {
            endpoint,
            providers,
            config,
            node_id,
            cluster_id,
        } => {
            let input = std::fs::read_to_string(&config)?;
            let config = BackendConfig::from_json(&input)?;
            let provider = load_provider(&providers, &config).await?;
            let engine = Arc::new(BackendEngine::start(config, provider).await?);
            let server = match LocalServer::bind(
                ServerOptions {
                    endpoint: endpoint.clone(),
                    node_id,
                    cluster_id,
                },
                Arc::clone(&engine),
            )
            .await
            {
                Ok(server) => server,
                Err(bind_error) => {
                    engine.shutdown().await?;
                    return Err(Box::<dyn Error>::from(bind_error));
                }
            };
            eprintln!("Patchouli daemon listening on {endpoint}");
            server.run().await?;
        }
        Command::Provide {
            listen,
            database,
            token_env,
        } => {
            let token = std::env::var(&token_env)
                .map_err(|_| format!("environment variable {token_env:?} is not set"))?;
            let provider: Arc<dyn Provider> = Arc::new(SqliteProvider::open(database).await?);
            let recovery = provider.initialize().await?;
            if let Err(error) = provider.health_check().await {
                provider.shutdown().await?;
                return Err(error.into());
            }
            let app = remote_provider_router(Arc::clone(&provider), token, recovery)?;
            let listener = tokio::net::TcpListener::bind(listen).await?;
            eprintln!("Patchouli remote provider listening on {listen}");
            let result = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = tokio::signal::ctrl_c().await;
                })
                .await;
            let shutdown = provider.shutdown().await;
            result?;
            shutdown?;
        }
        Command::Status { endpoint } => {
            let mut client =
                LocalClient::connect(&endpoint, "patchouli-cli", env!("CARGO_PKG_VERSION")).await?;
            println!("{}", serde_json::to_string_pretty(&client.status().await?)?);
        }
        Command::Checkpoint { endpoint } => {
            let mut client =
                LocalClient::connect(&endpoint, "patchouli-cli", env!("CARGO_PKG_VERSION")).await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&client.checkpoint().await?)?
            );
        }
        Command::Stop { endpoint } => {
            let mut client =
                LocalClient::connect(&endpoint, "patchouli-cli", env!("CARGO_PKG_VERSION")).await?;
            let result = client.shutdown().await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Command::Config {
            command: ConfigCommand::Check { path, providers },
        } => {
            let input = std::fs::read_to_string(&path)?;
            let config = BackendConfig::from_json(&input)?;
            if let Some(providers) = providers {
                let provider_input = std::fs::read_to_string(&providers)?;
                patchouli_server::ProviderConfig::from_json(&provider_input, &config)?;
            }
            println!("valid: {}", path.display());
        }
    }
    Ok(())
}
