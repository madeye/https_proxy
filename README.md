# https_proxy

A stealth HTTPS forward proxy in Rust. It auto-obtains TLS certificates via Let's Encrypt, authenticates users with basic auth, and disguises itself as a normal web server — returning a fake nginx 404 to scanners, browsers, and unauthorized clients.

## Features

- **Automatic TLS** — Certificates issued and renewed via ACME (TLS-ALPN-01 on port 443, no port 80 needed)
- **Stealth mode** — Non-proxy requests and failed auth both return an identical nginx-style 404
- **CONNECT tunneling** — Full HTTPS tunnel support for proxying TLS traffic
- **HTTP forwarding** — Plain HTTP proxy requests forwarded to upstream servers
- **Multi-user auth** — Basic auth with multiple username/password pairs

## Build

```bash
cargo build --release
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
  cache_dir: "/var/lib/https-proxy/acme"
users:
  - username: "alice"
    password: "hunter2"
stealth:
  server_name: "nginx/1.24.0"
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

## Usage

```bash
# Start the proxy (requires port 443 and a DNS record pointing to this server)
./target/release/https_proxy --config config.yaml

# Use as HTTPS proxy
curl --proxy https://alice:hunter2@proxy.example.com:443 https://httpbin.org/ip

# Probe the server directly — looks like nginx
curl https://proxy.example.com/
# => 404 Not Found (Server: nginx/1.24.0)

# Wrong credentials — same 404, no information leak
curl --proxy https://wrong:creds@proxy.example.com:443 https://example.com
# => 404 Not Found
```

## How It Works

1. All connections terminate TLS with a valid Let's Encrypt certificate
2. Requests without an absolute URI or CONNECT method are treated as probes → fake 404
3. Proxy requests with missing or invalid credentials → same fake 404
4. Authenticated CONNECT requests → TCP tunnel via HTTP upgrade + bidirectional copy
5. Authenticated HTTP requests → forwarded to upstream with proxy headers stripped

## License

MIT
