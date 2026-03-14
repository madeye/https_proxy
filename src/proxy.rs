//! HTTP CONNECT tunneling and plain HTTP forwarding.
//!
//! - [`handle_connect`]: Upgrades the client connection and tunnels bytes
//!   bidirectionally to the target via [`tokio::io::copy_bidirectional`].
//! - [`handle_forward`]: Strips proxy headers and forwards the request via
//!   a pooled HTTP client (or manual connection with TCP Fast Open).

use anyhow::Context;
use http_body_util::{Either, Full};
use hyper::body::{Bytes, Incoming};
use hyper::{Request, Response, StatusCode};
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tracing::{error, info};

use crate::net;
use crate::ProxyBody;

/// Buffer size per direction for CONNECT tunnels (128 KiB).
/// 16x the default 8 KiB — matches TLS record size and reduces syscall count.
const TUNNEL_BUF_SIZE: usize = 128 * 1024;

/// Global pooled HTTP/1.1 client for forward proxying (non-TFO path).
static POOLED_CLIENT: std::sync::OnceLock<Client<hyper_util::client::legacy::connect::HttpConnector, Incoming>> = std::sync::OnceLock::new();

fn get_pooled_client() -> &'static Client<hyper_util::client::legacy::connect::HttpConnector, Incoming> {
    POOLED_CLIENT.get_or_init(|| {
        Client::builder(TokioExecutor::new())
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .pool_max_idle_per_host(32)
            .build_http()
    })
}

/// Handle an HTTP `CONNECT` request by establishing a TCP tunnel.
///
/// Returns `200 OK` immediately to the client, then spawns a background task
/// that upgrades the connection and copies bytes bidirectionally between the
/// client and the target host. Optionally uses TCP Fast Open for the outgoing
/// connection when `fast_open` is `true`.
pub async fn handle_connect(
    req: Request<Incoming>,
    fast_open: bool,
) -> anyhow::Result<Response<ProxyBody>> {
    let authority = req
        .uri()
        .authority()
        .map(|a| a.to_string())
        .unwrap_or_else(|| {
            // CONNECT host:port comes in the URI directly
            req.uri().to_string()
        });

    let addr = if authority.contains(':') {
        authority.clone()
    } else {
        format!("{authority}:443")
    };

    info!("CONNECT tunnel to {addr}");

    // Spawn a task that upgrades the connection and tunnels data.
    tokio::spawn(async move {
        match hyper::upgrade::on(req).await {
            Ok(upgraded) => {
                let mut client = TokioIo::new(upgraded);
                match net::connect(&addr, fast_open).await {
                    Ok(mut target) => {
                        if let Err(e) =
                            tokio::io::copy_bidirectional_with_sizes(&mut client, &mut target, TUNNEL_BUF_SIZE, TUNNEL_BUF_SIZE).await
                        {
                            error!("tunnel {addr} io error: {e}");
                        }
                    }
                    Err(e) => {
                        error!("failed to connect to {addr}: {e}");
                    }
                }
            }
            Err(e) => {
                error!("upgrade failed for {addr}: {e}");
            }
        }
    });

    // Return 200 to signal the client that the tunnel is established.
    Ok(Response::builder()
        .status(StatusCode::OK)
        .body(Either::Left(Full::new(Bytes::new())))
        .unwrap())
}

/// Handle a plain HTTP forward proxy request with an absolute URI.
///
/// For the common case (`fast_open = false`), uses a pooled HTTP client that
/// reuses connections across requests. For `fast_open = true`, uses manual
/// connection setup with TCP Fast Open.
///
/// The response body is streamed directly from the upstream server without
/// buffering the entire body in memory.
pub async fn handle_forward(
    mut req: Request<Incoming>,
    fast_open: bool,
) -> anyhow::Result<Response<ProxyBody>> {
    let uri = req.uri().clone();

    info!("forward {} {}", req.method(), uri);

    // Strip hop-by-hop / proxy headers.
    let headers = req.headers_mut();
    headers.remove("proxy-authorization");
    headers.remove("proxy-connection");

    if fast_open {
        // TFO path: manual connection (pooled client doesn't support custom connectors yet)
        let host = uri
            .authority()
            .context("missing authority in forward request")?
            .to_string();

        let port = uri.port_u16().unwrap_or(match uri.scheme_str() {
            Some("https") => 443,
            _ => 80,
        });

        let addr = if host.contains(':') {
            host.clone()
        } else {
            format!("{host}:{port}")
        };

        // Rewrite the URI to path-only form for the upstream request.
        let path_and_query = uri
            .path_and_query()
            .map(|pq| pq.to_string())
            .unwrap_or_else(|| "/".to_string());
        *req.uri_mut() = path_and_query.parse()?;

        let stream = net::connect(&addr, true)
            .await
            .with_context(|| format!("connect to {addr}"))?;
        let io = TokioIo::new(stream);

        let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
            .await
            .context("upstream handshake")?;

        tokio::spawn(async move {
            if let Err(e) = conn.await {
                error!("upstream connection error: {e}");
            }
        });

        let resp = sender
            .send_request(req)
            .await
            .context("upstream send_request")?;

        let (parts, body) = resp.into_parts();
        Ok(Response::from_parts(parts, Either::Right(body)))
    } else {
        // Pooled client path: connection reuse, automatic URI handling
        let client = get_pooled_client();
        let resp = client
            .request(req)
            .await
            .context("pooled client request")?;

        let (parts, body) = resp.into_parts();
        Ok(Response::from_parts(parts, Either::Right(body)))
    }
}
