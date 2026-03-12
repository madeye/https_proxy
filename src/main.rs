use std::sync::Arc;

use anyhow::Context;
use clap::{Parser, Subcommand};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::Request;
use hyper_util::rt::TokioIo;
use tracing::{error, info};

use https_proxy::config::Config;
use https_proxy::handle_request;

#[derive(Parser)]
#[command(name = "https-proxy", about = "Stealth HTTPS forward proxy")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Interactive TUI to create config.yaml
    Setup {
        /// Output config file path
        #[arg(short, long, default_value = "config.yaml")]
        output: String,
    },
    /// Start the proxy server
    Run {
        /// Path to config file
        #[arg(short, long, default_value = "config.yaml")]
        config: String,
    },
    /// Install as a systemd background service (Linux only, requires root)
    Install {
        /// Path to config file
        #[arg(short, long, default_value = "config.yaml")]
        config: String,
    },
    /// Uninstall the systemd service (Linux only, requires root)
    Uninstall,
}

fn main() -> anyhow::Result<()> {
    tokio_rustls::rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install default CryptoProvider");

    let cli = Cli::parse();

    match cli.command {
        Some(Command::Setup { output }) => https_proxy::setup::run_setup(output),
        Some(Command::Run { config }) => run_server(config),
        Some(Command::Install { config }) => https_proxy::service::install_service(config),
        Some(Command::Uninstall) => https_proxy::service::uninstall_service(),
        None => run_server("config.yaml".into()),
    }
}

/// Start the proxy server: bind the listener, set up ACME, and accept connections.
#[tokio::main]
async fn run_server(config_path: String) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = Config::load(&config_path).context("failed to load config")?;
    let shared = Arc::new(config.clone());

    info!("starting proxy on {}", config.listen);

    let listener = https_proxy::net::create_listener(&config.listen, config.fast_open).await?;

    let acme = https_proxy::tls::build_acme_acceptor(&config)?;
    let acceptor = acme.acceptor;
    let rustls_config = acme.rustls_config;

    loop {
        let (tcp_stream, peer_addr) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let rustls_config = rustls_config.clone();
        let shared = shared.clone();

        tokio::spawn(async move {
            let tls_stream = match acceptor.accept(tcp_stream).await {
                Ok(Some(start)) => match start.into_stream(rustls_config).await {
                    Ok(stream) => stream,
                    Err(e) => {
                        error!("{peer_addr}: TLS handshake error: {e}");
                        return;
                    }
                },
                Ok(None) => return,
                Err(e) => {
                    error!("{peer_addr}: accept error: {e}");
                    return;
                }
            };

            let io = TokioIo::new(tls_stream);
            let shared = shared.clone();

            let service = service_fn(move |req: Request<Incoming>| {
                let shared = shared.clone();
                async move { handle_request(req, &shared).await }
            });

            if let Err(e) =
                hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new())
                    .http1()
                    .preserve_header_case(true)
                    .title_case_headers(true)
                    .http2()
                    .max_concurrent_streams(250)
                    .enable_connect_protocol()
                    .serve_connection_with_upgrades(io, service)
                    .await
            {
                error!("{peer_addr}: connection error: {e}");
            }
        });
    }
}
