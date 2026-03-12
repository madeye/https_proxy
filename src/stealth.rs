use hyper::{Method, Request, Response, StatusCode};
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};

/// Returns true if the request looks like a proxy request (absolute URI or CONNECT).
pub fn is_proxy_request(req: &Request<Incoming>) -> bool {
    if req.method() == Method::CONNECT {
        return true;
    }
    // Absolute URI (has authority) means proxy request
    req.uri().authority().is_some()
}

/// Return a fake 404 that looks like a normal nginx server.
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
