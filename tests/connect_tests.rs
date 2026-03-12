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
async fn test_connect_tunnel_to_echo_server() {
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

    let echo_url = format!("http://127.0.0.1:{}/hello", echo.addr.port());
    let resp = client.get(&echo_url).send().await.unwrap();

    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["uri"], "/hello");
    assert_eq!(body["method"], "GET");
}
