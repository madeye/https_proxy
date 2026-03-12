//! TLS termination with automatic ACME certificate provisioning.
//!
//! Uses [`tokio_rustls_acme`] to obtain and renew Let's Encrypt certificates
//! via the TLS-ALPN-01 challenge. ACME challenge connections are handled
//! transparently by the [`AcmeAcceptor`]; regular connections proceed
//! through the normal TLS handshake with the provisioned certificate.

use std::sync::Arc;

use futures::StreamExt;
use tokio_rustls::rustls::version::{TLS12, TLS13};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls_acme::caches::DirCache;
use tokio_rustls_acme::{AcmeAcceptor, AcmeConfig};

use crate::config::Config;

/// Components needed to accept TLS connections with ACME support.
pub struct AcmeSetup {
    /// Acceptor that intercepts ACME challenges and passes regular connections through.
    pub acceptor: AcmeAcceptor,
    /// TLS server configuration using the ACME-managed certificate resolver.
    pub rustls_config: Arc<ServerConfig>,
}

/// Build an [`AcmeAcceptor`] and TLS [`ServerConfig`] from the proxy configuration.
///
/// Spawns a background task that drives the ACME state machine, handling
/// certificate issuance and renewal events.
pub fn build_acme_acceptor(config: &Config) -> anyhow::Result<AcmeSetup> {
    let domain = config.domain.clone();
    let cache_dir = config.acme.cache_dir.clone();
    let acme_config = AcmeConfig::new([domain])
        .contact_push(format!("mailto:{}", config.acme.email))
        .cache(DirCache::new(cache_dir))
        .directory_lets_encrypt(!config.acme.staging);

    let mut state = acme_config.state();
    let acceptor = state.acceptor();
    let resolver = state.resolver();

    let mut rustls_config = ServerConfig::builder_with_protocol_versions(&[&TLS13, &TLS12])
        .with_no_client_auth()
        .with_cert_resolver(resolver);
    rustls_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    let rustls_config = Arc::new(rustls_config);

    // Spawn the ACME event loop to drive cert issuance/renewal.
    tokio::spawn(async move {
        loop {
            match state.next().await {
                Some(Ok(event)) => {
                    tracing::info!("ACME event: {:?}", event);
                }
                Some(Err(e)) => {
                    tracing::error!("ACME error: {:?}", e);
                }
                None => break,
            }
        }
    });

    Ok(AcmeSetup {
        acceptor,
        rustls_config,
    })
}
