// Copyright The ranger Contributors

//! 保留的旧 Runtime 本地 Unix IPC 客户端协议；不是新底盘 socket 合同。

#[cfg(target_os = "linux")]
use std::ffi::CString;
use std::io::{self, Read, Write};
#[cfg(target_os = "linux")]
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};
#[cfg(target_os = "linux")]
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub const PROTOCOL: &str = "ranger.framework.runtime.v1";
pub const MAX_FRAME: usize = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Status,
    Probe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok,
    Busy,
    Invalid,
    Unavailable,
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub protocol: String,
    pub request_id: u64,
    pub operation: Operation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeStatus {
    pub instance_id: String,
    pub backend: String,
    pub worker_count: u32,
    pub probe_count: u64,
    pub ready: bool,
    pub motion_capable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub protocol: String,
    pub request_id: u64,
    pub outcome: Outcome,
    pub runtime: Option<RuntimeStatus>,
}

pub fn read_frame<T: DeserializeOwned>(stream: &mut impl Read) -> io::Result<T> {
    let mut length = [0_u8; 4];
    stream.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_FRAME {
        return Err(invalid_data("frame exceeds MAX_FRAME"));
    }

    let mut payload = vec![0_u8; length];
    stream.read_exact(&mut payload)?;
    serde_json::from_slice(&payload)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

pub fn write_frame<T: Serialize>(stream: &mut impl Write, value: &T) -> io::Result<()> {
    let frame = encode_frame(value)?;
    stream.write_all(&frame)
}

pub fn call(
    socket: &Path,
    operation: Operation,
    request_id: u64,
    timeout: Duration,
) -> io::Result<Response> {
    if timeout.is_zero() {
        return Err(timed_out());
    }
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "timeout is too large"))?;
    let mut stream = connect_until(socket, deadline)?;
    let socket_timeout = remaining(deadline)?;
    stream.set_read_timeout(Some(socket_timeout))?;
    stream.set_write_timeout(Some(socket_timeout))?;

    let request = Request {
        protocol: PROTOCOL.into(),
        request_id,
        operation,
    };
    write_frame_until(&mut stream, &request, deadline)?;
    let response: Response = read_frame_until(&mut stream, deadline)?;
    if response.protocol != PROTOCOL {
        return Err(invalid_data("response protocol does not match"));
    }
    if response.request_id != request_id {
        return Err(invalid_data("response request_id does not match"));
    }
    Ok(response)
}

/// Returns the connected peer's effective UID on Linux through `SO_PEERCRED`.
///
/// Other operating systems return [`io::ErrorKind::Unsupported`].
#[cfg(target_os = "linux")]
pub fn peer_uid(stream: &UnixStream) -> io::Result<u32> {
    use std::mem::{size_of, zeroed};

    // SAFETY: `libc::ucred` is a plain C output structure and all-zero is a valid
    // initial buffer for `getsockopt` to fill.
    let mut credentials: libc::ucred = unsafe { zeroed() };
    let mut length = size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: the file descriptor is borrowed from a live UnixStream, and the
    // pointers and length describe writable `libc::ucred` storage exactly.
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut length,
        )
    };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    if length as usize != size_of::<libc::ucred>() {
        return Err(invalid_data("SO_PEERCRED returned an unexpected size"));
    }
    Ok(credentials.uid)
}

/// Returns [`io::ErrorKind::Unsupported`] because `SO_PEERCRED` is Linux-only.
#[cfg(not(target_os = "linux"))]
pub fn peer_uid(_stream: &UnixStream) -> io::Result<u32> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "peer UID requires Linux SO_PEERCRED",
    ))
}

fn encode_frame<T: Serialize>(value: &T) -> io::Result<Vec<u8>> {
    let payload = serde_json::to_vec(value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    if payload.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "frame exceeds MAX_FRAME",
        ));
    }

    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

/// Serializes and writes one frame before the caller-provided absolute deadline.
pub fn write_frame_until<T: Serialize>(
    stream: &mut UnixStream,
    value: &T,
    deadline: Instant,
) -> io::Result<()> {
    let frame = encode_frame(value)?;
    write_all_until(stream, &frame, deadline)
}

/// Reads and deserializes one frame before the caller-provided absolute deadline.
pub fn read_frame_until<T: DeserializeOwned>(
    stream: &mut UnixStream,
    deadline: Instant,
) -> io::Result<T> {
    let mut length = [0_u8; 4];
    read_exact_until(stream, &mut length, deadline)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_FRAME {
        return Err(invalid_data("frame exceeds MAX_FRAME"));
    }

    let mut payload = vec![0_u8; length];
    read_exact_until(stream, &mut payload, deadline)?;
    serde_json::from_slice(&payload)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn read_exact_until(
    stream: &mut UnixStream,
    buffer: &mut [u8],
    deadline: Instant,
) -> io::Result<()> {
    let mut read = 0;
    while read < buffer.len() {
        stream.set_read_timeout(Some(remaining(deadline)?))?;
        match stream.read(&mut buffer[read..]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "connection closed",
                ));
            }
            Ok(count) => read += count,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                return Err(timed_out());
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn write_all_until(stream: &mut UnixStream, buffer: &[u8], deadline: Instant) -> io::Result<()> {
    let mut written = 0;
    while written < buffer.len() {
        stream.set_write_timeout(Some(remaining(deadline)?))?;
        match stream.write(&buffer[written..]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "connection closed",
                ));
            }
            Ok(count) => written += count,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                return Err(timed_out());
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// Connects one Unix stream without exceeding `deadline` on Linux.
///
/// This is transport-only: it does not send a request or retry a request.
/// The deadline guarantee depends on Linux nonblocking Unix sockets, `poll`,
/// `SO_ERROR`, and `getpeername`; other targets use a blocking connect.
#[cfg(target_os = "linux")]
pub fn connect_until(socket: &Path, deadline: Instant) -> io::Result<UnixStream> {
    let path = CString::new(socket.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "socket path contains NUL"))?;
    let path_bytes = path.as_bytes_with_nul();
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if path_bytes.len() > address.sun_path.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "socket path is too long",
        ));
    }
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    // SAFETY: `sun_path` is an inline fixed-size byte buffer, and the length
    // check above guarantees that the copy stays within that buffer.
    let destination = unsafe {
        std::slice::from_raw_parts_mut(address.sun_path.as_mut_ptr().cast(), address.sun_path.len())
    };
    destination[..path_bytes.len()].copy_from_slice(path_bytes);
    let address_length =
        (std::mem::offset_of!(libc::sockaddr_un, sun_path) + path_bytes.len()) as libc::socklen_t;

    loop {
        let descriptor = nonblocking_socket()?;
        // SAFETY: address points to initialized storage for the stated length.
        let result = unsafe {
            libc::connect(
                descriptor.as_raw_fd(),
                (&raw const address).cast(),
                address_length,
            )
        };
        if result == 0 {
            match wait_for_connect(descriptor.as_raw_fd(), deadline) {
                Ok(()) => {
                    return into_blocking_stream(descriptor, deadline);
                }
                Err(wait_error) if wait_error.raw_os_error() == Some(libc::ENOTCONN) => {
                    wait_for_retry(deadline)?;
                    continue;
                }
                Err(wait_error) => return Err(wait_error),
            }
        }

        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            Some(libc::EINPROGRESS | libc::EALREADY | libc::EAGAIN) => {
                match wait_for_connect(descriptor.as_raw_fd(), deadline) {
                    Ok(()) => {
                        return into_blocking_stream(descriptor, deadline);
                    }
                    Err(wait_error) if wait_error.raw_os_error() == Some(libc::ENOTCONN) => {
                        wait_for_retry(deadline)?;
                    }
                    Err(wait_error) => return Err(wait_error),
                }
            }
            Some(libc::ENOTCONN) => wait_for_retry(deadline)?,
            _ => return Err(error),
        }
    }
}

/// Connects one Unix stream without exceeding `deadline` on Linux.
///
/// This is transport-only: it does not send a request or retry a request.
/// The deadline guarantee depends on Linux nonblocking Unix sockets, `poll`,
/// `SO_ERROR`, and `getpeername`; other targets use a blocking connect.
#[cfg(not(target_os = "linux"))]
pub fn connect_until(socket: &Path, deadline: Instant) -> io::Result<UnixStream> {
    let stream = UnixStream::connect(socket)?;
    remaining(deadline)?;
    Ok(stream)
}

#[cfg(target_os = "linux")]
fn wait_for_connect(descriptor: libc::c_int, deadline: Instant) -> io::Result<()> {
    loop {
        let timeout = poll_timeout(remaining(deadline)?);
        let mut events = libc::pollfd {
            fd: descriptor,
            events: libc::POLLOUT,
            revents: 0,
        };
        // SAFETY: events points to one initialized pollfd and timeout is finite.
        let result = unsafe { libc::poll(&raw mut events, 1, timeout) };
        if result == 0 {
            return Err(timed_out());
        }
        if result == -1 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if events.revents & libc::POLLNVAL != 0 {
            return Err(invalid_data("poll rejected the socket descriptor"));
        }
        if events.revents & (libc::POLLOUT | libc::POLLERR | libc::POLLHUP) != 0 {
            let error = socket_error(descriptor)?;
            if error == 0 && peer_is_connected(descriptor)? {
                remaining(deadline)?;
                return Ok(());
            }
            if error == 0 {
                wait_for_retry(deadline)?;
                continue;
            }
            return Err(io::Error::from_raw_os_error(error));
        }
    }
}

#[cfg(target_os = "linux")]
fn into_blocking_stream(descriptor: OwnedFd, deadline: Instant) -> io::Result<UnixStream> {
    // SAFETY: descriptor is valid and fcntl only reads its current status flags.
    let flags = unsafe { libc::fcntl(descriptor.as_raw_fd(), libc::F_GETFL) };
    if flags == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: descriptor is valid and this clears only the nonblocking status flag.
    if unsafe {
        libc::fcntl(
            descriptor.as_raw_fd(),
            libc::F_SETFL,
            flags & !libc::O_NONBLOCK,
        )
    } == -1
    {
        return Err(io::Error::last_os_error());
    }
    remaining(deadline)?;
    // SAFETY: ownership transfers exactly once from OwnedFd to UnixStream.
    Ok(unsafe { UnixStream::from_raw_fd(descriptor.into_raw_fd()) })
}

#[cfg(target_os = "linux")]
fn peer_is_connected(descriptor: libc::c_int) -> io::Result<bool> {
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of_val(&address) as libc::socklen_t;
    // SAFETY: the descriptor is live, and address/length point to valid writable storage.
    let result = unsafe {
        libc::getpeername(
            descriptor,
            (&mut address as *mut libc::sockaddr_un).cast(),
            &mut length,
        )
    };
    if result == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ENOTCONN) {
        return Ok(false);
    }
    Err(error)
}

#[cfg(target_os = "linux")]
fn nonblocking_socket() -> io::Result<OwnedFd> {
    // SAFETY: arguments are valid Linux constants and the returned descriptor
    // is immediately adopted by OwnedFd on success.
    let descriptor = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0,
        )
    };
    if descriptor == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `descriptor` is newly created and has not been adopted elsewhere.
    Ok(unsafe { OwnedFd::from_raw_fd(descriptor) })
}

#[cfg(target_os = "linux")]
fn wait_for_retry(deadline: Instant) -> io::Result<()> {
    let delay = remaining(deadline)?.min(Duration::from_millis(1));
    // SAFETY: no descriptors are supplied and the finite timeout only delays this thread.
    let result = unsafe { libc::poll(std::ptr::null_mut(), 0, poll_timeout(delay)) };
    if result == -1 {
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
    remaining(deadline).map(|_| ())
}

#[cfg(target_os = "linux")]
fn socket_error(descriptor: libc::c_int) -> io::Result<libc::c_int> {
    let mut error = 0;
    let mut length = std::mem::size_of_val(&error) as libc::socklen_t;
    // SAFETY: the descriptor is live, and error/length point to valid writable storage.
    let result = unsafe {
        libc::getsockopt(
            descriptor,
            libc::SOL_SOCKET,
            libc::SO_ERROR,
            (&mut error as *mut libc::c_int).cast(),
            &mut length,
        )
    };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    if length as usize != std::mem::size_of_val(&error) {
        return Err(invalid_data("SO_ERROR returned an unexpected size"));
    }
    Ok(error)
}

#[cfg(target_os = "linux")]
fn poll_timeout(duration: Duration) -> libc::c_int {
    duration.as_millis().max(1).min(libc::c_int::MAX as u128) as libc::c_int
}

fn remaining(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(timed_out)
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn timed_out() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "runtime IPC deadline elapsed")
}
