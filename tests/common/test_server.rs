#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;

use https_proxy::config::{AcmeConfig, Config, StealthConfig, UserConfig};

use super::tls_fixture::{generate_test_tls, TlsFixture};

pub struct TestServer {
    pub addr: SocketAddr,
    pub ca_pem: String,
    shutdown: CancellationToken,
}

impl TestServer {
    pub async fn start(users: Vec<UserConfig>) -> Self {
        // Ensure CryptoProvider is installed (idempotent)
        let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();

        let tls = generate_test_tls();
        Self::start_with_tls(users, tls).await
    }

    async fn start_with_tls(users: Vec<UserConfig>, tls: TlsFixture) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let config = Arc::new(Config {
            listen: addr.to_string(),
            domain: "localhost".to_string(),
            acme: AcmeConfig {
                email: "test@example.com".to_string(),
                staging: true,
                cache_dir: std::path::PathBuf::from("/tmp/test-acme-cache"),
            },
            users,
            stealth: StealthConfig::default(),
            fast_open: false,
        });

        let acceptor = TlsAcceptor::from(tls.server_config);
        let shutdown = CancellationToken::new();
        let token = shutdown.clone();

        tokio::spawn(async move {
            if let Err(e) =
                https_proxy::serve_with_tls_acceptor(listener, acceptor, config, token).await
            {
                eprintln!("test server error: {e}");
            }
        });

        TestServer {
            addr,
            ca_pem: tls.ca_pem,
            shutdown,
        }
    }

    pub fn proxy_url(&self) -> String {
        format!("https://127.0.0.1:{}", self.addr.port())
    }

    pub fn reqwest_client(&self) -> reqwest::Client {
        let cert = reqwest::tls::Certificate::from_pem(self.ca_pem.as_bytes()).unwrap();
        reqwest::Client::builder()
            .add_root_certificate(cert)
            .danger_accept_invalid_certs(false)
            .build()
            .unwrap()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}
