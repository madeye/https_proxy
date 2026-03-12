use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use hyper::body::Incoming;
use hyper::Request;

use crate::config::UserConfig;

pub fn check_proxy_auth(req: &Request<Incoming>, users: &[UserConfig]) -> bool {
    let header = req
        .headers()
        .get("proxy-authorization")
        .and_then(|v| v.to_str().ok());

    let credentials = match header {
        Some(h) if h.starts_with("Basic ") => {
            let encoded = &h[6..];
            match STANDARD.decode(encoded) {
                Ok(decoded) => String::from_utf8(decoded).ok(),
                Err(_) => None,
            }
        }
        _ => None,
    };

    match credentials {
        Some(cred) => cred
            .split_once(':')
            .is_some_and(|(user, pass)| users.iter().any(|u| u.username == user && u.password == pass)),
        None => false,
    }
}
