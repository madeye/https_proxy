//! Network utilities with optional TCP Fast Open (TFO) support.
//!
//! Provides [`create_listener`] and [`connect`] as drop-in replacements for
//! [`tokio::net::TcpListener::bind`] and [`tokio::net::TcpStream::connect`]
//! that set the `TCP_FASTOPEN` / `TCP_FASTOPEN_CONNECT` socket options when
//! enabled. Supports both Linux and macOS.
//!
//! On Linux, ensure TFO is enabled at the kernel level:
//! ```text
//! sysctl -w net.ipv4.tcp_fastopen=3
//! ```

use std::net::SocketAddr;

use anyhow::Context;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use tokio::net::{TcpListener, TcpStream};
use tracing::info;

/// Default TCP Fast Open queue length for the listening socket.
const TFO_QUEUE_LEN: i32 = 256;

/// Create a [`TcpListener`] bound to `addr`, optionally with TCP Fast Open.
///
/// When `fast_open` is `true`, sets `TCP_FASTOPEN` on the socket before
/// binding so the kernel can accept TFO connections.
pub async fn create_listener(addr: &str, fast_open: bool) -> anyhow::Result<TcpListener> {
    let sock_addr: SocketAddr = addr
        .parse()
        .with_context(|| format!("invalid listen address: {addr}"))?;

    let domain = if sock_addr.is_ipv6() {
        Domain::IPV6
    } else {
        Domain::IPV4
    };

    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))
        .context("failed to create socket")?;

    socket.set_reuse_address(true)?;

    if fast_open {
        set_tcp_fastopen(&socket, TFO_QUEUE_LEN)?;
        info!("TCP Fast Open enabled on listener (queue={TFO_QUEUE_LEN})");
    }

    socket
        .bind(&SockAddr::from(sock_addr))
        .with_context(|| format!("bind {addr}"))?;
    socket
        .listen(1024)
        .with_context(|| format!("listen {addr}"))?;
    socket.set_nonblocking(true)?;

    TcpListener::from_std(socket.into()).context("convert to tokio TcpListener")
}

/// Connect to a remote address, optionally with TCP Fast Open.
///
/// When `fast_open` is `true`, sets `TCP_FASTOPEN_CONNECT` (Linux) or
/// `TCP_FASTOPEN` (macOS) on the socket before connecting so the kernel
/// can send data in the SYN packet.
pub async fn connect(addr: &str, fast_open: bool) -> anyhow::Result<TcpStream> {
    if !fast_open {
        return TcpStream::connect(addr)
            .await
            .with_context(|| format!("connect to {addr}"));
    }

    // Resolve the address first
    let sock_addr: SocketAddr = tokio::net::lookup_host(addr)
        .await
        .with_context(|| format!("resolve {addr}"))?
        .next()
        .with_context(|| format!("no addresses for {addr}"))?;

    let domain = if sock_addr.is_ipv6() {
        Domain::IPV6
    } else {
        Domain::IPV4
    };

    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))
        .context("failed to create socket")?;

    // Enable TFO on the connecting socket
    set_tcp_fastopen_connect(&socket)?;

    socket.set_nonblocking(true)?;

    // Initiate non-blocking connect (kernel may use TFO if supported)
    match socket.connect(&SockAddr::from(sock_addr)) {
        Ok(()) => {}
        Err(e) if e.raw_os_error() == Some(libc::EINPROGRESS) => {}
        Err(e) => return Err(e).with_context(|| format!("connect to {addr}")),
    }

    let std_stream: std::net::TcpStream = socket.into();
    let stream = TcpStream::from_std(std_stream).context("convert to tokio TcpStream")?;

    // Wait for the connection to complete
    stream
        .writable()
        .await
        .with_context(|| format!("connect to {addr}"))?;

    // Check for connection error
    if let Some(e) = stream.take_error()? {
        return Err(e).with_context(|| format!("connect to {addr}"));
    }

    Ok(stream)
}

/// Set `TCP_FASTOPEN` on a listening socket.
///
/// On Linux, `queue_len` sets the maximum pending TFO connections.
/// On macOS, a boolean flag is used instead (queue length is ignored).
fn set_tcp_fastopen(socket: &Socket, queue_len: i32) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::io::AsRawFd;
        let fd = socket.as_raw_fd();
        let val = queue_len;
        let ret = unsafe {
            libc::setsockopt(
                fd,
                libc::IPPROTO_TCP,
                libc::TCP_FASTOPEN,
                &val as *const _ as *const libc::c_void,
                std::mem::size_of_val(&val) as libc::socklen_t,
            )
        };
        if ret != 0 {
            return Err(std::io::Error::last_os_error()).context("setsockopt TCP_FASTOPEN");
        }
    }

    #[cfg(target_os = "macos")]
    {
        // macOS uses a different constant (TCP_FASTOPEN = 0x105)
        use std::os::unix::io::AsRawFd;
        let fd = socket.as_raw_fd();
        let val: i32 = 1; // enable flag, not queue length
        let ret = unsafe {
            libc::setsockopt(
                fd,
                libc::IPPROTO_TCP,
                0x105, // TCP_FASTOPEN on macOS
                &val as *const _ as *const libc::c_void,
                std::mem::size_of_val(&val) as libc::socklen_t,
            )
        };
        if ret != 0 {
            return Err(std::io::Error::last_os_error()).context("setsockopt TCP_FASTOPEN");
        }
    }

    let _ = queue_len;
    Ok(())
}

/// Set `TCP_FASTOPEN_CONNECT` on an outgoing socket.
///
/// On Linux, uses `TCP_FASTOPEN_CONNECT` (option 30) which allows
/// `connect()` to send data in the SYN packet. On macOS, reuses the
/// `TCP_FASTOPEN` flag (`0x105`).
fn set_tcp_fastopen_connect(socket: &Socket) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::io::AsRawFd;
        let fd = socket.as_raw_fd();
        let val: i32 = 1;
        // TCP_FASTOPEN_CONNECT (30) enables TFO for connect()
        let ret = unsafe {
            libc::setsockopt(
                fd,
                libc::IPPROTO_TCP,
                30, // TCP_FASTOPEN_CONNECT
                &val as *const _ as *const libc::c_void,
                std::mem::size_of_val(&val) as libc::socklen_t,
            )
        };
        if ret != 0 {
            return Err(std::io::Error::last_os_error())
                .context("setsockopt TCP_FASTOPEN_CONNECT");
        }
    }

    #[cfg(target_os = "macos")]
    {
        use std::os::unix::io::AsRawFd;
        let fd = socket.as_raw_fd();
        let val: i32 = 1;
        let ret = unsafe {
            libc::setsockopt(
                fd,
                libc::IPPROTO_TCP,
                0x105, // TCP_FASTOPEN on macOS
                &val as *const _ as *const libc::c_void,
                std::mem::size_of_val(&val) as libc::socklen_t,
            )
        };
        if ret != 0 {
            return Err(std::io::Error::last_os_error()).context("setsockopt TCP_FASTOPEN");
        }
    }

    Ok(())
}
