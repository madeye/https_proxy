mod common;

use std::time::Instant;

use common::data_server::DataServer;
use common::echo_server::EchoServer;
use common::test_server::TestServer;

/// 64 MB for CONNECT tunnel throughput test.
const CONNECT_SIZE: usize = 64 * 1024 * 1024;
/// 32 MB for HTTP forward throughput test.
const FORWARD_SIZE: usize = 32 * 1024 * 1024;
/// Minimum acceptable throughput on localhost (MB/s).
const MIN_THROUGHPUT_MBPS: f64 = 100.0;
/// Number of small requests for the request rate test.
const REQUEST_COUNT: usize = 500;

#[tokio::test]
async fn test_connect_tunnel_throughput() {
    let data = DataServer::start(CONNECT_SIZE).await;
    let server = TestServer::start_no_auth().await;

    let proxy = reqwest::Proxy::all(server.proxy_url())
        .unwrap();

    let ca_cert = reqwest::tls::Certificate::from_pem(server.ca_pem.as_bytes()).unwrap();

    let client = reqwest::Client::builder()
        .proxy(proxy)
        .add_root_certificate(ca_cert)
        .build()
        .unwrap();

    let url = format!("http://127.0.0.1:{}/data", data.addr.port());

    // Warm up
    let _ = client.get(&url).send().await.unwrap().bytes().await.unwrap();

    let start = Instant::now();
    let resp = client.get(&url).send().await.unwrap();
    let body = resp.bytes().await.unwrap();
    let elapsed = start.elapsed();

    assert_eq!(body.len(), CONNECT_SIZE);

    let mbps = (CONNECT_SIZE as f64 / (1024.0 * 1024.0)) / elapsed.as_secs_f64();
    eprintln!(
        "CONNECT tunnel: {:.1} MB in {:.3}s = {:.1} MB/s",
        CONNECT_SIZE as f64 / (1024.0 * 1024.0),
        elapsed.as_secs_f64(),
        mbps
    );
    assert!(
        mbps > MIN_THROUGHPUT_MBPS,
        "CONNECT throughput too low: {mbps:.1} MB/s (minimum {MIN_THROUGHPUT_MBPS} MB/s)"
    );
}

#[tokio::test]
async fn test_http_forward_throughput() {
    let data = DataServer::start(FORWARD_SIZE).await;
    let server = TestServer::start_no_auth().await;

    let proxy = reqwest::Proxy::http(server.proxy_url())
        .unwrap();

    let ca_cert = reqwest::tls::Certificate::from_pem(server.ca_pem.as_bytes()).unwrap();

    let client = reqwest::Client::builder()
        .proxy(proxy)
        .add_root_certificate(ca_cert)
        .build()
        .unwrap();

    let url = format!("http://127.0.0.1:{}/data", data.addr.port());

    // Warm up
    let _ = client.get(&url).send().await.unwrap().bytes().await.unwrap();

    let start = Instant::now();
    let resp = client.get(&url).send().await.unwrap();
    let body = resp.bytes().await.unwrap();
    let elapsed = start.elapsed();

    assert_eq!(body.len(), FORWARD_SIZE);

    let mbps = (FORWARD_SIZE as f64 / (1024.0 * 1024.0)) / elapsed.as_secs_f64();
    eprintln!(
        "HTTP forward: {:.1} MB in {:.3}s = {:.1} MB/s",
        FORWARD_SIZE as f64 / (1024.0 * 1024.0),
        elapsed.as_secs_f64(),
        mbps
    );
    assert!(
        mbps > MIN_THROUGHPUT_MBPS,
        "HTTP forward throughput too low: {mbps:.1} MB/s (minimum {MIN_THROUGHPUT_MBPS} MB/s)"
    );
}

#[tokio::test]
async fn test_http_forward_request_rate() {
    let echo = EchoServer::start().await;
    let server = TestServer::start_no_auth().await;

    let proxy = reqwest::Proxy::http(server.proxy_url())
        .unwrap();

    let ca_cert = reqwest::tls::Certificate::from_pem(server.ca_pem.as_bytes()).unwrap();

    let client = reqwest::Client::builder()
        .proxy(proxy)
        .add_root_certificate(ca_cert)
        .build()
        .unwrap();

    let url = format!("http://127.0.0.1:{}/ping", echo.addr.port());

    // Warm up
    let _ = client.get(&url).send().await.unwrap().bytes().await.unwrap();

    let start = Instant::now();
    for _ in 0..REQUEST_COUNT {
        let resp = client.get(&url).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let _ = resp.bytes().await.unwrap();
    }
    let elapsed = start.elapsed();

    let rps = REQUEST_COUNT as f64 / elapsed.as_secs_f64();
    eprintln!(
        "HTTP forward: {REQUEST_COUNT} requests in {:.3}s = {:.0} req/s",
        elapsed.as_secs_f64(),
        rps
    );
    // With connection pooling, expect at least 200 req/s on localhost
    assert!(
        rps > 200.0,
        "Request rate too low: {rps:.0} req/s (minimum 200 req/s)"
    );
}
