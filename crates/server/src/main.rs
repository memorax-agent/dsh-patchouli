use std::{
    error::Error,
    fs::OpenOptions,
    io::Write,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

use clap::{Parser, Subcommand};
use patchouli_backend::{BackendConfig, BackendEngine};
use patchouli_provider::Provider;
use patchouli_provider_remote::remote_provider_router;
use patchouli_provider_sqlite::SqliteProvider;
use patchouli_server::{LocalClient, LocalServer, ServerOptions, load_provider, shutdown_signal};

#[derive(Debug, Parser)]
#[command(
    name = "patchouli-db",
    version,
    about = "Patchouli local daemon and control CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a local backend home with default policy and provider configuration.
    Init {
        #[arg(long)]
        root: PathBuf,
    },
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
        Command::Init { root } => initialize_home(&root)?,
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
                    if let Err(shutdown_error) = engine.shutdown().await {
                        return Err(format!(
                            "{bind_error}; backend shutdown also failed: {shutdown_error}"
                        )
                        .into());
                    }
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
            serve_provider(listen, database, token).await?;
        }
        Command::Status { endpoint } => {
            let mut client =
                LocalClient::connect(&endpoint, "patchouli-db-cli", env!("CARGO_PKG_VERSION"))
                    .await?;
            println!("{}", serde_json::to_string_pretty(&client.status().await?)?);
        }
        Command::Checkpoint { endpoint } => {
            let mut client =
                LocalClient::connect(&endpoint, "patchouli-db-cli", env!("CARGO_PKG_VERSION"))
                    .await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&client.checkpoint().await?)?
            );
        }
        Command::Stop { endpoint } => {
            let mut client =
                LocalClient::connect(&endpoint, "patchouli-db-cli", env!("CARGO_PKG_VERSION"))
                    .await?;
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

fn initialize_home(root: &Path) -> Result<(), Box<dyn Error>> {
    create_private_dir(root)?;
    create_private_dir(&root.join("data"))?;
    create_private_dir(&root.join("run"))?;
    write_if_missing(
        &root.join("config.json"),
        include_bytes!("../../../config/patchouli.default.json"),
    )?;
    write_if_missing(
        &root.join("providers.json"),
        include_bytes!("../../../config/providers.local.json"),
    )?;
    write_if_missing(
        &root.join("patchouli.schema.json"),
        include_bytes!("../../../config/patchouli.schema.json"),
    )?;
    write_if_missing(
        &root.join("providers.schema.json"),
        include_bytes!("../../../config/providers.schema.json"),
    )?;

    let policy_path = root.join("config.json");
    let provider_path = root.join("providers.json");
    let policy = BackendConfig::from_json(&std::fs::read_to_string(&policy_path)?)?;
    let providers = patchouli_server::ProviderConfig::from_json(
        &std::fs::read_to_string(&provider_path)?,
        &policy,
    )?;
    let database = match providers.providers.get("local") {
        Some(patchouli_server::ProviderDefinition::Local { database }) => database,
        _ => unreachable!("validated provider configuration requires local storage"),
    };
    SqliteProvider::validate_existing_storage(if database.is_absolute() {
        database.clone()
    } else {
        root.join(database)
    })?;
    println!("initialized Patchouli home: {}", root.display());
    println!("policy: {}", policy_path.display());
    println!("providers: {}", provider_path.display());
    Ok(())
}

async fn serve_provider(
    listen: SocketAddr,
    database: PathBuf,
    token: String,
) -> Result<(), Box<dyn Error>> {
    let provider: Arc<dyn Provider> = Arc::new(SqliteProvider::open(database).await?);
    let result: Result<(), Box<dyn Error>> = async {
        let recovery = provider.initialize().await?;
        provider.health_check().await?;
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let app = remote_provider_router(Arc::clone(&provider), token, recovery, shutdown_rx)?;
        let listener = tokio::net::TcpListener::bind(listen).await?;
        let bound_address = listener.local_addr()?;
        eprintln!("Patchouli remote provider listening on {bound_address}");
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let signal = shutdown_signal().await;
                let _ = shutdown_tx.send(true);
                if let Err(error) = signal {
                    eprintln!("failed to listen for shutdown signal: {error}");
                }
            })
            .await?;
        Ok(())
    }
    .await;
    let shutdown = provider.shutdown().await;
    match (result, shutdown) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error.into()),
        (Err(error), Err(shutdown_error)) => {
            Err(format!("{error}; provider shutdown also failed: {shutdown_error}").into())
        }
    }
}

fn create_private_dir(path: &Path) -> Result<(), std::io::Error> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => return validate_private_permissions(path),
        Ok(_) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("{} exists and is not a directory", path.display()),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(path)?;
    validate_private_permissions(path)
}

fn write_if_missing(path: &Path, contents: &[u8]) -> Result<(), std::io::Error> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    match options.open(path) {
        Ok(mut file) => file.write_all(contents),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            validate_private_permissions(path)
        }
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn validate_private_permissions(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;

    let mode = std::fs::metadata(path)?.permissions().mode();
    if mode & 0o077 == 0 {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "{} is accessible by other users (mode {:03o}); expected no group/other permissions",
                path.display(),
                mode & 0o777
            ),
        ))
    }
}

#[cfg(not(unix))]
fn validate_private_permissions(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}
