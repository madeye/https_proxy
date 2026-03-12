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

fn make_proxy_client(server: &TestServer) -> reqwest::Client {
    let proxy = reqwest::Proxy::all(server.proxy_url())
        .unwrap()
        .basic_auth("testuser", "testpass");

    let ca_cert = reqwest::tls::Certificate::from_pem(server.ca_pem.as_bytes()).unwrap();

    reqwest::Client::builder()
        .proxy(proxy)
        .add_root_certificate(ca_cert)
        .build()
        .unwrap()
}

#[tokio::test]
async fn test_proxy_authorization_stripped() {
    let echo = EchoServer::start().await;
    let server = TestServer::start(test_users()).await;
    let client = make_proxy_client(&server);

    let echo_url = format!("http://127.0.0.1:{}/headers", echo.addr.port());
    let resp = client.get(&echo_url).send().await.unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();

    let headers = body["headers"].as_object().unwrap();
    assert!(
        !headers.contains_key("proxy-authorization"),
        "proxy-authorization should be stripped"
    );
}

#[tokio::test]
async fn test_custom_headers_preserved() {
    let echo = EchoServer::start().await;
    let server = TestServer::start(test_users()).await;
    let client = make_proxy_client(&server);

    let echo_url = format!("http://127.0.0.1:{}/headers", echo.addr.port());
    let resp = client
        .get(&echo_url)
        .header("x-custom", "foo")
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();

    let headers = body["headers"].as_object().unwrap();
    assert_eq!(
        headers.get("x-custom").and_then(|v| v.as_str()),
        Some("foo"),
        "custom headers should be preserved"
    );
}
