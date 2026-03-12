//! Stealth HTTPS forward proxy.
//!
//! A forward proxy that auto-obtains TLS certificates via ACME/Let's Encrypt
//! and disguises itself as a normal nginx web server. Unauthorized or non-proxy
//! requests receive a fake nginx 404 page.
//!
//! # Request flow
//!
//! 1. TLS termination with automatic ACME cert provisioning ([`tls`])
//! 2. Stealth gate — non-proxy traffic gets a fake 404 ([`stealth`])
//! 3. Auth gate — invalid credentials get the same fake 404 ([`auth`])
//! 4. CONNECT tunneling or HTTP forwarding to the target ([`proxy`])

pub mod auth;
pub mod config;
pub mod net;
pub(crate) mod proxy;
pub mod service;
pub mod setup;
pub mod stealth;
pub mod tls;

use std::sync::Arc;

use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Method, Request, Response};
use hyper_util::rt::TokioIo;
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;
use tracing::error;

use crate::config::Config;

/// Route an incoming request through stealth detection, auth, and proxy handling.
pub async fn handle_request(
    req: Request<Incoming>,
    config: &Config,
) -> Result<Response<Full<Bytes>>, anyhow::Error> {
    if !stealth::is_proxy_request(&req) {
        return Ok(stealth::fake_404(&config.stealth.server_name));
    }

    #[cfg(feature = "test-support")]
    let auth_ok = config.skip_auth || auth::check_proxy_auth(&req, &config.users);
    #[cfg(not(feature = "test-support"))]
    let auth_ok = auth::check_proxy_auth(&req, &config.users);

    if !auth_ok {
        return Ok(stealth::proxy_auth_required(&config.stealth.server_name));
    }

    if req.method() == Method::CONNECT {
        proxy::handle_connect(req, config.fast_open).await
    } else {
        proxy::handle_forward(req, config.fast_open).await
    }
}

/// Run the proxy server using a pre-built [`TlsAcceptor`] (bypasses ACME).
///
/// This entry point is used by integration tests that supply self-signed
/// certificates via `rcgen` instead of going through Let's Encrypt.
pub async fn serve_with_tls_acceptor(
    listener: tokio::net::TcpListener,
    acceptor: TlsAcceptor,
    config: Arc<Config>,
    shutdown: CancellationToken,
) -> anyhow::Result<()> {
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => return Ok(()),
            result = listener.accept() => {
                let (tcp_stream, peer_addr) = result?;
                let acceptor = acceptor.clone();
                let config = config.clone();
                let shutdown = shutdown.clone();

                tokio::spawn(async move {
                    let tls_stream = match acceptor.accept(tcp_stream).await {
                        Ok(stream) => stream,
                        Err(e) => {
                            error!("{peer_addr}: TLS handshake error: {e}");
                            return;
                        }
                    };

                    let io = TokioIo::new(tls_stream);
                    let config = config.clone();

                    let service = service_fn(move |req: Request<Incoming>| {
                        let config = config.clone();
                        async move { handle_request(req, &config).await }
                    });

                    let mut builder = hyper_util::server::conn::auto::Builder::new(
                        hyper_util::rt::TokioExecutor::new(),
                    );
                    builder
                        .http1()
                        .preserve_header_case(true)
                        .title_case_headers(true);
                    builder
                        .http2()
                        .max_concurrent_streams(250)
                        .enable_connect_protocol();
                    let conn = builder.serve_connection_with_upgrades(io, service);

                    tokio::select! {
                        _ = shutdown.cancelled() => {}
                        result = conn => {
                            if let Err(e) = result {
                                error!("{peer_addr}: connection error: {e}");
                            }
                        }
                    }
                });
            }
        }
    }
}
