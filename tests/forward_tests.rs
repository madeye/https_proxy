mod common;

use common::echo_server::EchoServer;
use common::test_server::TestServer;
use https_proxy::config::UserConfig;

fn test_users() -> Vec<UserConfig> {
    vec![UserConfig {
        username: "testuser".to_string(),
        password: "testpass".to_string(),
    }]
}

#[tokio::test]
async fn test_http_forward_to_echo_server() {
    let echo = EchoServer::start().await;
    let server = TestServer::start(test_users()).await;

    let proxy = reqwest::Proxy::all(server.proxy_url())
        .unwrap()
        .basic_auth("testuser", "testpass");

    let ca_cert = reqwest::tls::Certificate::from_pem(server.ca_pem.as_bytes()).unwrap();

    let client = reqwest::Client::builder()
        .proxy(proxy)
        .add_root_certificate(ca_cert)
        .build()
        .unwrap();

    let echo_url = format!("http://127.0.0.1:{}/test/path", echo.addr.port());
    let resp = client.get(&echo_url).send().await.unwrap();

    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_forward_rewrites_uri_to_path() {
    let echo = EchoServer::start().await;
    let server = TestServer::start(test_users()).await;

    let proxy = reqwest::Proxy::all(server.proxy_url())
        .unwrap()
        .basic_auth("testuser", "testpass");

    let ca_cert = reqwest::tls::Certificate::from_pem(server.ca_pem.as_bytes()).unwrap();

    let client = reqwest::Client::builder()
        .proxy(proxy)
        .add_root_certificate(ca_cert)
        .build()
        .unwrap();

    let echo_url = format!("http://127.0.0.1:{}/some/path", echo.addr.port());
    let resp = client.get(&echo_url).send().await.unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();

    // The echo server should see path-only URI, not absolute
    let uri = body["uri"].as_str().unwrap();
    assert_eq!(uri, "/some/path", "URI should be rewritten to path-only");
    assert!(
        !uri.starts_with("http://"),
        "URI should not be absolute form"
    );
}
