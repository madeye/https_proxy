use std::sync::Arc;

use rcgen::generate_simple_self_signed;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::ServerConfig;

pub struct TlsFixture {
    pub server_config: Arc<ServerConfig>,
    pub ca_pem: String,
}

pub fn generate_test_tls() -> TlsFixture {
    let subject_alt_names = vec!["127.0.0.1".to_string(), "localhost".to_string()];
    let certified_key = generate_simple_self_signed(subject_alt_names).unwrap();

    let ca_pem = certified_key.cert.pem();

    let cert_der = CertificateDer::from(certified_key.cert.der().to_vec());
    let key_der = PrivateKeyDer::try_from(certified_key.signing_key.serialize_der()).unwrap();

    let mut server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)
        .unwrap();
    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    TlsFixture {
        server_config: Arc::new(server_config),
        ca_pem,
    }
}
