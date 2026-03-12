mod auth;
mod config;
mod proxy;
mod stealth;
mod tls;

use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Method, Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tracing::{error, info};

use crate::config::Config;

#[derive(Parser)]
#[command(name = "https-proxy", about = "Stealth HTTPS forward proxy")]
struct Cli {
    /// Path to config file
    #[arg(short, long, default_value = "config.yaml")]
    config: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    let config = Config::load(&cli.config).context("failed to load config")?;
    let shared = Arc::new(config.clone());

    info!("starting proxy on {}", config.listen);

    let listener = TcpListener::bind(&config.listen)
        .await
        .with_context(|| format!("bind {}", config.listen))?;

    let acme = tls::build_acme_acceptor(&config)?;
    let acceptor = acme.acceptor;
    let rustls_config = acme.rustls_config;

    loop {
        let (tcp_stream, peer_addr) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let rustls_config = rustls_config.clone();
        let shared = shared.clone();

        tokio::spawn(async move {
            // Let the ACME acceptor handle TLS-ALPN-01 challenges and regular TLS.
            let tls_stream = match acceptor.accept(tcp_stream).await {
                Ok(Some(start)) => match start.into_stream(rustls_config).await {
                    Ok(stream) => stream,
                    Err(e) => {
                        error!("{peer_addr}: TLS handshake error: {e}");
                        return;
                    }
                },
                Ok(None) => {
                    // Was an ACME challenge connection, already handled.
                    return;
                }
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

            if let Err(e) = hyper::server::conn::http1::Builder::new()
                .preserve_header_case(true)
                .title_case_headers(true)
                .serve_connection(io, service)
                .with_upgrades()
                .await
            {
                error!("{peer_addr}: connection error: {e}");
            }
        });
    }
}

async fn handle_request(
    req: Request<Incoming>,
    config: &Config,
) -> Result<Response<Full<Bytes>>, anyhow::Error> {
    // Stealth: if not a proxy request, return fake 404.
    if !stealth::is_proxy_request(&req) {
        return Ok(stealth::fake_404(&config.stealth.server_name));
    }

    // Auth check: return same fake 404 on failure to stay stealthy.
    if !auth::check_proxy_auth(&req, &config.users) {
        return Ok(stealth::fake_404(&config.stealth.server_name));
    }

    // Route to appropriate handler.
    if req.method() == Method::CONNECT {
        proxy::handle_connect(req).await
    } else {
        proxy::handle_forward(req).await
    }
}
