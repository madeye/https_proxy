#![allow(dead_code)]

use std::net::SocketAddr;

use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

/// A simple HTTP server that returns a configurable number of zero-filled bytes.
/// Used for throughput testing.
pub struct DataServer {
    pub addr: SocketAddr,
    shutdown: CancellationToken,
}

impl DataServer {
    /// Start a data server that returns `size` bytes of zeros for any request.
    pub async fn start(size: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let shutdown = CancellationToken::new();
        let token = shutdown.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    result = listener.accept() => {
                        let (stream, _) = match result {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                        let token = token.clone();
                        tokio::spawn(async move {
                            let io = TokioIo::new(stream);
                            let service = service_fn(move |_req: Request<Incoming>| {
                                let data = vec![0u8; size];
                                async move {
                                    Ok::<_, hyper::Error>(
                                        Response::builder()
                                            .status(200)
                                            .header("Content-Type", "application/octet-stream")
                                            .header("Content-Length", size.to_string())
                                            .body(Full::new(Bytes::from(data)))
                                            .unwrap(),
                                    )
                                }
                            });
                            let conn = http1::Builder::new().serve_connection(io, service);
                            tokio::select! {
                                _ = token.cancelled() => {}
                                result = conn => {
                                    if let Err(e) = result {
                                        eprintln!("data server error: {e}");
                                    }
                                }
                            }
                        });
                    }
                }
            }
        });

        DataServer { addr, shutdown }
    }
}

impl Drop for DataServer {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}
