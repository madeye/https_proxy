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

fn curl_available() -> bool {
    std::process::Command::new("curl")
        .arg("--version")
        .output()
        .is_ok()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_curl_forward_through_proxy() {
    if !curl_available() {
        eprintln!("curl not found, skipping");
        return;
    }

    let echo = EchoServer::start().await;
    let server = TestServer::start(test_users()).await;

    let echo_url = format!("http://127.0.0.1:{}/curl-test", echo.addr.port());
    let proxy_url = server.proxy_url();

    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("curl")
            .arg("--silent")
            .arg("--max-time")
            .arg("5")
            .arg("--proxy")
            .arg(&proxy_url)
            .arg("--proxy-user")
            .arg("testuser:testpass")
            .arg("--proxy-insecure")
            .arg(&echo_url)
            .output()
            .unwrap()
    })
    .await
    .unwrap();

    assert!(
        output.status.success(),
        "curl failed (code {:?}): {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let body: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("curl output should be valid JSON");
    assert_eq!(body["uri"], "/curl-test");
    assert_eq!(body["method"], "GET");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_curl_no_auth_gets_407() {
    if !curl_available() {
        eprintln!("curl not found, skipping");
        return;
    }

    let echo = EchoServer::start().await;
    let server = TestServer::start(test_users()).await;

    let echo_url = format!("http://127.0.0.1:{}/curl-test", echo.addr.port());
    let proxy_url = server.proxy_url();

    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("curl")
            .arg("--silent")
            .arg("--max-time")
            .arg("5")
            .arg("--proxy")
            .arg(&proxy_url)
            .arg("--proxy-insecure")
            .arg("--write-out")
            .arg("%{http_code}")
            .arg("--output")
            .arg("/dev/null")
            .arg(&echo_url)
            .output()
            .unwrap()
    })
    .await
    .unwrap();

    let status_code = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        status_code, "407",
        "missing auth on proxy request should get 407"
    );
}

/// Curl the proxy directly (not as a proxy) with HTTP/1.1 — should get stealth 404.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_curl_direct_http1_gets_stealth_404() {
    if !curl_available() {
        eprintln!("curl not found, skipping");
        return;
    }

    let server = TestServer::start(test_users()).await;
    let url = format!("https://127.0.0.1:{}/", server.addr.port());

    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("curl")
            .arg("--silent")
            .arg("--max-time")
            .arg("5")
            .arg("--insecure")
            .arg("--http1.1")
            .arg("--write-out")
            .arg("%{http_code}")
            .arg("--output")
            .arg("/dev/null")
            .arg(&url)
            .output()
            .unwrap()
    })
    .await
    .unwrap();

    let status_code = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        status_code, "404",
        "direct HTTP/1.1 request should get stealth 404"
    );
}

/// Curl the proxy directly (not as a proxy) with HTTP/2 — should get stealth 404, not 407.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_curl_direct_request_gets_stealth_404() {
    if !curl_available() {
        eprintln!("curl not found, skipping");
        return;
    }

    let server = TestServer::start(test_users()).await;

    // Direct request to the proxy (no --proxy flag) — this is a non-proxy request
    let url = format!("https://127.0.0.1:{}/", server.addr.port());

    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("curl")
            .arg("--silent")
            .arg("--max-time")
            .arg("5")
            .arg("--insecure")
            .arg("--write-out")
            .arg("%{http_code}")
            .arg("--output")
            .arg("/dev/null")
            .arg(&url)
            .output()
            .unwrap()
    })
    .await
    .unwrap();

    let status_code = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        status_code, "404",
        "direct (non-proxy) request should get stealth 404, not 407"
    );
}
