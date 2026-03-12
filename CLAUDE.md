# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
cargo build              # dev build
cargo build --release    # release build (stripped, LTO)
cargo check              # type-check without building
cargo clippy             # lint
cargo test                     # run all integration tests
cargo test --test stealth_tests  # run specific test file
```

## Architecture

Stealth HTTPS forward proxy that auto-obtains TLS certs via ACME/Let's Encrypt and disguises itself as a normal nginx web server.

### Request Flow

1. **TLS accept** (`tls.rs`): ACME acceptor handles TLS-ALPN-01 challenges transparently; regular connections get a TLS stream with auto-renewed Let's Encrypt cert.
2. **Stealth gate** (`stealth.rs`): Non-proxy requests (no absolute URI, no CONNECT) → fake nginx 404.
3. **Auth gate** (`auth.rs`): Invalid/missing `Proxy-Authorization: Basic ...` → same fake 404 (not 407, to avoid revealing it's a proxy).
4. **CONNECT tunnel** (`proxy.rs`): `hyper::upgrade::on()` + `tokio::io::copy_bidirectional` to target.
5. **HTTP forward** (`proxy.rs`): Rewrites absolute URI to path-only, strips proxy headers, forwards via `hyper::client::conn::http1`.

### Key Design Decisions

- **Stealth for non-proxy traffic**: Non-proxy requests (no absolute URI, no CONNECT) return nginx 404. Proxy requests with missing/wrong auth get 407 so real clients (Chrome) can authenticate.
- **hyper 1.x with upgrades**: `http1::Builder` must use `.with_upgrades()` for CONNECT tunneling to work.
- **Proxy detection**: `req.uri().authority().is_some()` (absolute URI) or `Method::CONNECT`.
- **ACME on port 443 only**: Uses TLS-ALPN-01 challenge type, no port 80 listener needed.
- **tokio-rustls-acme v0.6 API**: `AcmeState` is a `Stream`; drive it with `StreamExt::next()` in a spawned task. `start_handshake.into_stream(rustls_config)` requires an `Arc<ServerConfig>` built with `state.resolver()`.

## Config

Copy `config.example.yaml` to `config.yaml`. Structure: `listen`, `domain`, `acme` (email, staging bool, cache_dir), `users` (username/password list), `stealth` (server_name).
