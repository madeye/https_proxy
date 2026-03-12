#![allow(dead_code)]

use std::collections::HashMap;
use std::net::SocketAddr;

use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

pub struct EchoServer {
    pub addr: SocketAddr,
    shutdown: CancellationToken,
}

impl EchoServer {
    pub async fn start() -> Self {
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
                            let service = service_fn(echo_handler);
                            let conn = http1::Builder::new()
                                .serve_connection(io, service);
                            tokio::select! {
                                _ = token.cancelled() => {}
                                result = conn => {
                                    if let Err(e) = result {
                                        eprintln!("echo server error: {e}");
                                    }
                                }
                            }
                        });
                    }
                }
            }
        });

        EchoServer { addr, shutdown }
    }
}

impl Drop for EchoServer {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

async fn echo_handler(req: Request<Incoming>) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let mut headers: HashMap<String, String> = HashMap::new();
    for (name, value) in req.headers() {
        headers.insert(
            name.to_string(),
            value.to_str().unwrap_or("<binary>").to_string(),
        );
    }

    let info = serde_json::json!({
        "method": req.method().to_string(),
        "uri": req.uri().to_string(),
        "headers": headers,
    });

    let body = serde_json::to_string_pretty(&info).unwrap();

    Ok(Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(body)))
        .unwrap())
}
