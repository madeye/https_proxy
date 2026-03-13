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

fn chrome_path() -> Option<String> {
    // Fixed paths (macOS)
    let candidates = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
    ];
    for path in &candidates {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }
    // PATH-based lookup (Linux CI)
    for name in [
        "google-chrome-stable",
        "google-chrome",
        "chromium-browser",
        "chromium",
    ] {
        if std::process::Command::new(name)
            .arg("--version")
            .output()
            .is_ok()
        {
            return Some(name.to_string());
        }
    }
    None
}

/// Chrome fetches an HTTP URL through the HTTPS proxy (no-auth, HTTP forward path).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_chrome_http_forward_no_auth() {
    let chrome = match chrome_path() {
        Some(p) => p,
        None => {
            eprintln!("Chrome/Chromium not found, skipping");
            return;
        }
    };

    let echo = EchoServer::start().await;
    let server = TestServer::start_no_auth().await;

    let echo_url = format!("http://127.0.0.1:{}/chrome-test", echo.addr.port());
    let proxy_url = server.proxy_url();

    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new(&chrome)
            .arg("--headless=new")
            .arg("--disable-gpu")
            .arg("--no-sandbox")
            .arg("--disable-software-rasterizer")
            .arg("--timeout=10000")
            .arg(format!("--proxy-server={proxy_url}"))
            .arg("--ignore-certificate-errors")
            .arg("--dump-dom")
            .arg(&echo_url)
            .output()
            .unwrap()
    })
    .await
    .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stdout.contains("/chrome-test"),
        "Chrome should fetch the page through the proxy.\nSTDOUT: {stdout}\nSTDERR: {stderr}"
    );
}

/// Chrome sends CONNECT for HTTPS URLs (no-auth, tests H2 CONNECT tunnel).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_chrome_https_connect_no_auth() {
    let chrome = match chrome_path() {
        Some(p) => p,
        None => {
            eprintln!("Chrome/Chromium not found, skipping");
            return;
        }
    };

    let server = TestServer::start_no_auth().await;
    let proxy_url = server.proxy_url();

    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new(&chrome)
            .arg("--headless=new")
            .arg("--disable-gpu")
            .arg("--no-sandbox")
            .arg("--disable-software-rasterizer")
            .arg("--timeout=15000")
            .arg(format!("--proxy-server={proxy_url}"))
            .arg("--ignore-certificate-errors")
            .arg("--dump-dom")
            .arg("https://example.com/")
            .output()
            .unwrap()
    })
    .await
    .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stdout.contains("ERR_TUNNEL_CONNECTION_FAILED"),
        "Chrome CONNECT tunnel should not fail.\nSTDOUT (truncated): {}\nSTDERR: {stderr}",
        &stdout[..stdout.len().min(500)]
    );
    assert!(
        stdout.contains("Example Domain"),
        "Chrome should fetch example.com through the CONNECT tunnel.\nSTDOUT (truncated): {}\nSTDERR: {stderr}",
        &stdout[..stdout.len().min(500)]
    );
}

/// When auth is required, Chrome gets a 407 Proxy-Authenticate challenge.
/// Headless Chrome can't complete the auth handshake without extensions,
/// so we verify it receives 407 (not 404) which enables interactive auth.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_chrome_gets_407_when_auth_required() {
    let chrome = match chrome_path() {
        Some(p) => p,
        None => {
            eprintln!("Chrome/Chromium not found, skipping");
            return;
        }
    };

    let server = TestServer::start(test_users()).await;
    let proxy_url = server.proxy_url();

    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new(&chrome)
            .arg("--headless=new")
            .arg("--disable-gpu")
            .arg("--no-sandbox")
            .arg("--disable-software-rasterizer")
            .arg("--timeout=10000")
            .arg(format!("--proxy-server={proxy_url}"))
            .arg("--ignore-certificate-errors")
            .arg("--dump-dom")
            .arg("https://example.com/")
            .output()
            .unwrap()
    })
    .await
    .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Chrome should show ERR_PROXY_AUTH_REQUESTED (got 407), not
    // ERR_TUNNEL_CONNECTION_FAILED (which would mean the proxy is broken).
    assert!(
        !stdout.contains("ERR_TUNNEL_CONNECTION_FAILED"),
        "Should get auth challenge, not tunnel failure"
    );
}

/// Chrome navigates directly to the proxy URL (not using it as a proxy).
/// Should get the stealth nginx 404 page, not a 407. Chrome uses HTTP/2
/// over TLS, which previously triggered a false positive in is_proxy_request.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_chrome_direct_visit_gets_stealth_404() {
    let chrome = match chrome_path() {
        Some(p) => p,
        None => {
            eprintln!("Chrome/Chromium not found, skipping");
            return;
        }
    };

    let server = TestServer::start(test_users()).await;

    // Navigate directly to the proxy (no --proxy-server flag)
    let url = format!("https://127.0.0.1:{}/", server.addr.port());

    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new(&chrome)
            .arg("--headless=new")
            .arg("--disable-gpu")
            .arg("--no-sandbox")
            .arg("--disable-software-rasterizer")
            .arg("--timeout=10000")
            .arg("--ignore-certificate-errors")
            .arg("--dump-dom")
            .arg(&url)
            .output()
            .unwrap()
    })
    .await
    .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stdout.contains("404 Not Found"),
        "Direct visit should see stealth 404 page, not 407.\nSTDOUT: {stdout}\nSTDERR: {stderr}"
    );
    assert!(
        stdout.contains("nginx/1.24.0"),
        "Stealth 404 should mimic nginx.\nSTDOUT: {stdout}\nSTDERR: {stderr}"
    );
}

/// Chrome navigates directly to a subpath on the proxy — should still get stealth 404.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_chrome_direct_visit_subpath_gets_stealth_404() {
    let chrome = match chrome_path() {
        Some(p) => p,
        None => {
            eprintln!("Chrome/Chromium not found, skipping");
            return;
        }
    };

    let server = TestServer::start(test_users()).await;

    let url = format!("https://127.0.0.1:{}/some/random/path", server.addr.port());

    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new(&chrome)
            .arg("--headless=new")
            .arg("--disable-gpu")
            .arg("--no-sandbox")
            .arg("--disable-software-rasterizer")
            .arg("--timeout=10000")
            .arg("--ignore-certificate-errors")
            .arg("--dump-dom")
            .arg(&url)
            .output()
            .unwrap()
    })
    .await
    .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stdout.contains("404 Not Found"),
        "Direct visit to subpath should see stealth 404.\nSTDOUT: {stdout}\nSTDERR: {stderr}"
    );
}
