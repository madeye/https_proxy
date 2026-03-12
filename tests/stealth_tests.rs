mod common;

use common::test_server::TestServer;
use https_proxy::config::UserConfig;

fn test_users() -> Vec<UserConfig> {
    vec![UserConfig {
        username: "testuser".to_string(),
        password: "testpass".to_string(),
    }]
}

#[tokio::test]
async fn test_non_proxy_request_gets_nginx_404() {
    let server = TestServer::start(test_users()).await;
    let client = server.reqwest_client();

    let resp = client
        .get(format!("{}/", server.proxy_url()))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
    assert_eq!(
        resp.headers().get("server").unwrap().to_str().unwrap(),
        "nginx/1.24.0"
    );
}

#[tokio::test]
async fn test_non_proxy_request_body_matches_nginx() {
    let server = TestServer::start(test_users()).await;
    let client = server.reqwest_client();

    let resp = client
        .get(format!("{}/", server.proxy_url()))
        .send()
        .await
        .unwrap();

    let body = resp.text().await.unwrap();
    assert!(body.contains("404 Not Found"));
    assert!(body.contains("nginx/1.24.0"));
}
