use anyhow::Context;
use hyper::body::Incoming;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use http_body_util::{Full, BodyExt};
use hyper::body::Bytes;
use tokio::net::TcpStream;
use tracing::{info, error};

/// Handle a CONNECT request: establish a TCP tunnel to the target.
pub async fn handle_connect(req: Request<Incoming>) -> anyhow::Result<Response<Full<Bytes>>> {
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
                match TcpStream::connect(&addr).await {
                    Ok(mut target) => {
                        if let Err(e) =
                            tokio::io::copy_bidirectional(&mut client, &mut target).await
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
        .body(Full::new(Bytes::new()))
        .unwrap())
}

/// Handle a plain HTTP forward proxy request (absolute URI).
pub async fn handle_forward(mut req: Request<Incoming>) -> anyhow::Result<Response<Full<Bytes>>> {
    let uri = req.uri().clone();
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

    info!("forward {} {} -> {addr}", req.method(), uri);

    // Rewrite the URI to path-only form for the upstream request.
    let path_and_query = uri
        .path_and_query()
        .map(|pq| pq.to_string())
        .unwrap_or_else(|| "/".to_string());
    *req.uri_mut() = path_and_query.parse()?;

    // Strip hop-by-hop / proxy headers.
    let headers = req.headers_mut();
    headers.remove("proxy-authorization");
    headers.remove("proxy-connection");

    // Connect to the upstream server.
    let stream = TcpStream::connect(&addr)
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

    // Collect the upstream response body.
    let (parts, body) = resp.into_parts();
    let body_bytes = body
        .collect()
        .await
        .context("read upstream body")?
        .to_bytes();

    Ok(Response::from_parts(parts, Full::new(body_bytes)))
}
