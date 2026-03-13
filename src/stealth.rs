//! Stealth layer that hides the proxy from scanners and browsers.
//!
//! Non-proxy traffic (no absolute URI, no `CONNECT`) receives a nginx-style
//! 404 response, making the proxy indistinguishable from a misconfigured web
//! server. Proxy requests with missing/invalid auth get a `407` so that
//! real clients (e.g. Chrome) can send credentials.

use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, StatusCode, Version};

/// Returns `true` if the request is a proxy request.
///
/// A request is considered a proxy request if it uses the `CONNECT` method
/// or has an absolute URI (i.e., contains an authority component).
///
/// For HTTP/2, the `:authority` pseudo-header is always present in normal
/// requests, so `uri().authority()` alone is not sufficient. We only treat
/// HTTP/2 requests as proxy requests if they use CONNECT (extended CONNECT
/// per RFC 8441). For HTTP/1.x, an absolute-form URI (with authority)
/// indicates a proxy request.
pub fn is_proxy_request(req: &Request<Incoming>) -> bool {
    if req.method() == Method::CONNECT {
        return true;
    }
    // HTTP/2 always has :authority — only CONNECT is a proxy request
    if req.version() == Version::HTTP_2 {
        return false;
    }
    // HTTP/1.x: absolute URI (has authority) means proxy request
    req.uri().authority().is_some()
}

/// Build a fake 404 response mimicking nginx.
///
/// The response includes the configured `Server` header and an HTML body
/// identical to what nginx produces for a missing page.
pub fn fake_404(server_name: &str) -> Response<Full<Bytes>> {
    let body = concat!(
        "<html>\r\n",
        "<head><title>404 Not Found</title></head>\r\n",
        "<body>\r\n",
        "<center><h1>404 Not Found</h1></center>\r\n",
        "<hr><center>nginx/1.24.0</center>\r\n",
        "</body>\r\n",
        "</html>\r\n",
    );

    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header("Server", server_name)
        .header("Content-Type", "text/html")
        .header("Content-Length", body.len().to_string())
        .body(Full::new(Bytes::from(body)))
        .unwrap()
}

/// Build a 407 response requesting proxy authentication.
///
/// Sent when a proxy request (CONNECT or absolute URI) arrives without
/// valid credentials. The `Proxy-Authenticate` header tells clients like
/// Chrome to prompt for or resend credentials.
pub fn proxy_auth_required(server_name: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(StatusCode::PROXY_AUTHENTICATION_REQUIRED)
        .header("Server", server_name)
        .header("Proxy-Authenticate", "Basic realm=\"proxy\"")
        .header("Content-Length", "0")
        .body(Full::new(Bytes::new()))
        .unwrap()
}
