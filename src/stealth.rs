//! Stealth layer that hides the proxy from scanners and browsers.
//!
//! Non-proxy traffic (no absolute URI, no `CONNECT`) and failed auth both
//! receive an identical nginx-style 404 response, making the proxy
//! indistinguishable from a misconfigured web server.

use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, StatusCode};

/// Returns `true` if the request is a proxy request.
///
/// A request is considered a proxy request if it uses the `CONNECT` method
/// or has an absolute URI (i.e., contains an authority component).
pub fn is_proxy_request(req: &Request<Incoming>) -> bool {
    if req.method() == Method::CONNECT {
        return true;
    }
    // Absolute URI (has authority) means proxy request
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
