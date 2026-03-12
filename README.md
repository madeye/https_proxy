# https_proxy

A stealth HTTPS forward proxy in Rust. It auto-obtains TLS certificates via Let's Encrypt, authenticates users with basic auth, and disguises itself as a normal web server — returning a fake nginx 404 to scanners and non-proxy traffic.

## Features

- **Automatic TLS** — Certificates issued and renewed via ACME (TLS-ALPN-01 on port 443, no port 80 needed)
- **Stealth mode** — Non-proxy requests get an identical nginx-style 404; proxy requests with bad auth get a standard 407 so real clients (Chrome, curl) can authenticate
- **HTTP/2 support** — Full HTTP/2 with extended CONNECT protocol (RFC 8441) for browser compatibility
- **CONNECT tunneling** — Full HTTPS tunnel support for proxying TLS traffic
- **HTTP forwarding** — Plain HTTP proxy requests forwarded to upstream servers
- **Multi-user auth** — Basic auth with multiple username/password pairs
- **TUI setup wizard** — Interactive terminal UI to generate `config.yaml`

## Build

Requires Rust 1.70+ and a C compiler (for `aws-lc-sys`/`ring` crypto backends).

```bash
# Native release build (stripped, LTO enabled)
cargo build --release

# Cross-compile for Linux x86_64 from macOS (requires cargo-zigbuild + zig)
rustup target add x86_64-unknown-linux-gnu
cargo zigbuild --release --target x86_64-unknown-linux-gnu
```

### Prerequisites

**macOS:**
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# For cross-compilation to Linux
brew install zig
cargo install cargo-zigbuild
```

**Linux (Debian/Ubuntu):**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
apt install build-essential cmake
```

The release binary is stripped with LTO enabled (~3 MB).

## Configuration

Copy `config.example.yaml` to `config.yaml`:

```yaml
listen: "0.0.0.0:443"
domain: "proxy.example.com"
acme:
  email: "admin@example.com"
  staging: false
  cache_dir: "/var/lib/https_proxy/acme"
users:
  - username: "alice"
    password: "hunter2"
stealth:
  server_name: "nginx/1.24.0"
fast_open: true
```

| Field | Description |
|-------|-------------|
| `listen` | Bind address |
| `domain` | Domain for the ACME certificate |
| `acme.email` | Let's Encrypt contact email |
| `acme.staging` | Use staging environment (for testing) |
| `acme.cache_dir` | Directory to persist certificates |
| `users` | List of authorized proxy credentials |
| `stealth.server_name` | `Server` header in fake 404 responses |
| `fast_open` | Enable TCP Fast Open on listener and outgoing connections |

## Quick Start

```bash
# Generate config interactively
./target/release/https_proxy setup

# Or copy and edit the example
cp config.example.yaml config.yaml
```

## Usage

```bash
# Start the proxy (requires port 443 and a DNS record pointing to this server)
./target/release/https_proxy run --config config.yaml

# Or just run with default config.yaml
./target/release/https_proxy

# Use as HTTPS proxy
curl --proxy https://alice:hunter2@proxy.example.com:443 https://httpbin.org/ip

# Probe the server directly — looks like nginx
curl https://proxy.example.com/
# => 404 Not Found (Server: nginx/1.24.0)
```

## CLI

```
https_proxy [COMMAND]

Commands:
  setup      Interactive TUI to create config.yaml
  run        Start the proxy server (default if no command given)
  install    Install as a systemd background service (Linux, requires root)
  uninstall  Remove the systemd service
```

## How It Works

1. All connections terminate TLS with a valid Let's Encrypt certificate (HTTP/1.1 and HTTP/2)
2. Requests without an absolute URI or CONNECT method are treated as probes → fake nginx 404
3. Proxy requests with missing or invalid credentials → 407 with `Proxy-Authenticate` header (enables browser auth prompts)
4. Authenticated CONNECT requests → TCP tunnel via HTTP upgrade + bidirectional copy
5. Authenticated HTTP requests → forwarded to upstream with proxy headers stripped

## Testing

```bash
cargo test                       # run all integration tests
cargo test --test stealth_tests  # run specific test file
cargo test --test chrome_tests   # run Chrome/Chromium browser tests
```

The test suite covers stealth responses, auth gates, CONNECT tunneling, HTTP forwarding, header stripping, curl end-to-end, and Chrome headless browser compatibility.

## License

MIT
