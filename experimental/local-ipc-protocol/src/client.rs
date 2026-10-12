//! 实验性本地 IPC 的同步 Linux 客户端。
//!
//! 该客户端只编码本 crate 定义的 packet，不保存控制权、不重连、不重发 mutation，也不
//! 根据本地时间刷新原始 TTL。Runtime 通过 `SO_PEERCRED` 判定角色，客户端标签只是日志
//! 标签。

use std::{
    ffi::CString,
    io,
    mem::{size_of, zeroed},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::ffi::OsStrExt,
    },
    path::{Path, PathBuf},
    sync::{Arc, Weak},
    time::Duration,
};

use crate::{
    CLOCK_DOMAIN, ClientPacket, MockTarget, OperationState as WireOperationState, PROTOCOL,
    RealVehicleCapability, ServerPacket, encode,
};

const CONTROL_CONFIRMATION_POLL_NS: u64 = 1_000_000;

/// Linux 本地 IPC 客户端连接参数。
#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub socket_path: PathBuf,
    pub client_label: String,
    pub max_packet_bytes: usize,
    /// 单次同步 request 的 send/receive 上限；超时不推断请求未生效，调用方必须保持
    /// 原 operation 身份并按结果查询或标为 Unknown，不能自动重发 mutation。
    pub io_timeout: Duration,
}

/// 一次 packet 交换的错误。
#[derive(Debug)]
pub enum ClientError {
    Io(io::Error),
    Encode(serde_json::Error),
    Decode(serde_json::Error),
    InvalidConfig(&'static str),
    Protocol(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "本地 IPC I/O 失败：{error}"),
            Self::Encode(error) => write!(f, "本地 IPC 编码失败：{error}"),
            Self::Decode(error) => write!(f, "本地 IPC 解码失败：{error}"),
            Self::InvalidConfig(detail) => write!(f, "本地 IPC 客户端配置无效：{detail}"),
            Self::Protocol(detail) => write!(f, "Runtime 协议拒绝：{detail}"),
        }
    }
}

impl std::error::Error for ClientError {}

/// Acquire 或 Takeover 的只读周期应用确认结论。
///
/// `Unknown` 不说明控制一定未被授予；它只说明客户端未能在原始 deadline 前，以同一
/// session 的 Result 与 Status 同时确认应用事实。调用者不得重发原 mutation。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlEpochConfirmation {
    /// Result 已完成，且同一 Status 显示活动控制会话和调用方指定的精确 epoch。
    Confirmed { epoch: u64 },
    /// deadline、未知/失败结果、协议异常或 I/O 失败使确认无法成立。
    Unknown,
}

/// 已握手的同步 `SOCK_SEQPACKET` 客户端。
#[derive(Debug)]
pub struct LocalIpcClient {
    fd: Arc<OwnedFd>,
    max_packet_bytes: usize,
    hello: ServerPacket,
}

/// 可从持有 `LocalIpcClient` 的线程预先取得的连接中止句柄。
///
/// 它只对同一个 Unix socket 执行 `SHUT_RDWR`，使 Runtime 观察到该精确 session 关闭并
/// 撤销其自己的 lease；不编码 Stop packet、不触发全局安全锁存、不自动重连或重试。
/// 适配器在正常 request 被互斥锁阻塞时可持有此句柄，随后必须把在途请求标为 `Unknown`。
#[derive(Clone, Debug)]
pub struct LocalIpcAbortHandle {
    fd: Weak<OwnedFd>,
}

impl LocalIpcClient {
    /// 建立 `SOCK_SEQPACKET` 连接并完成唯一 Hello。Runtime 返回的 boot/session/role
    /// 绑定可由调用者保存；重启或重新连接不会恢复旧操作查询。
    pub fn connect(config: ClientConfig) -> Result<Self, ClientError> {
        if config.max_packet_bytes == 0 {
            return Err(ClientError::InvalidConfig("max_packet_bytes 必须大于零"));
        }
        if config.io_timeout.is_zero() {
            return Err(ClientError::InvalidConfig("io_timeout 必须大于零"));
        }
        let fd = connect_seqpacket(&config.socket_path, config.io_timeout)?;
        let mut client = Self {
            fd: Arc::new(fd),
            max_packet_bytes: config.max_packet_bytes,
            hello: ServerPacket::Error {
                code: crate::ErrorCode::NotAuthorized,
                detail: "握手尚未完成".into(),
            },
        };
        let response = client.request(ClientPacket::Hello {
            protocol: PROTOCOL.into(),
            clock_domain: CLOCK_DOMAIN.into(),
            client_label: config.client_label,
        })?;
        match response {
            ref hello @ ServerPacket::Hello {
                ref protocol,
                ref clock_domain,
                simulated: true,
                real_vehicle_capability: RealVehicleCapability::Unavailable,
                ..
            } if protocol == PROTOCOL && clock_domain == CLOCK_DOMAIN => {
                client.hello = hello.clone();
                Ok(client)
            }
            ServerPacket::Hello { .. } => Err(ClientError::Protocol(
                "Hello 的 protocol、clock_domain 或 mock 能力合同不匹配".into(),
            )),
            ServerPacket::Error { detail, .. } => Err(ClientError::Protocol(detail)),
            _ => Err(ClientError::Protocol("Hello 未返回会话身份".into())),
        }
    }

    /// 返回握手中固定的 Runtime boot/session/role 投影。
    pub fn hello(&self) -> &ServerPacket {
        &self.hello
    }

    /// 返回可跨线程使用的本连接中止句柄。
    ///
    /// 该句柄不取得 `&mut self`，所以能在另一个线程持有 client request mutex 时中止
    /// 阻塞的 `send` 或 `recv`。中止后的连接不可复用。
    pub fn abort_handle(&self) -> LocalIpcAbortHandle {
        LocalIpcAbortHandle {
            fd: Arc::downgrade(&self.fd),
        }
    }

    /// 中止本连接，使 Runtime 关闭并仅撤销该 session 的控制资格。
    ///
    /// 正在进行的 request 的结果因此不可确认，调用方必须记录 `Unknown` 并清除自己的
    /// 本地输入窗口；本方法不是 `Stop`，不改变其他 session 的 lease。
    pub fn abort(&self) -> Result<(), ClientError> {
        shutdown_connection(self.fd.as_raw_fd())
    }

    /// 严格的一发一收 packet 交换。调用者负责递增 request ID、绝对 TTL 和结果归属。
    pub fn request(&mut self, packet: ClientPacket) -> Result<ServerPacket, ClientError> {
        let bytes = encode(&packet).map_err(ClientError::Encode)?;
        let sent = unsafe {
            libc::send(
                self.fd.as_raw_fd(),
                bytes.as_ptr().cast(),
                bytes.len(),
                libc::MSG_NOSIGNAL,
            )
        };
        if sent != bytes.len() as isize {
            return Err(ClientError::Io(io::Error::last_os_error()));
        }
        let mut buffer = vec![0_u8; self.max_packet_bytes];
        let received = unsafe {
            libc::recv(
                self.fd.as_raw_fd(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                0,
            )
        };
        if received <= 0 {
            return Err(ClientError::Io(io::Error::last_os_error()));
        }
        serde_json::from_slice(&buffer[..received as usize]).map_err(ClientError::Decode)
    }

    pub fn status(&mut self) -> Result<ServerPacket, ClientError> {
        self.request(ClientPacket::Status {})
    }

    pub fn acquire(
        &mut self,
        request_id: u64,
        expected_epoch: u64,
        expires_at_ns: u64,
    ) -> Result<ServerPacket, ClientError> {
        self.request(ClientPacket::Acquire {
            request_id,
            expected_epoch,
            expires_at_ns,
        })
    }

    pub fn takeover(
        &mut self,
        request_id: u64,
        expected_epoch: u64,
        expires_at_ns: u64,
    ) -> Result<ServerPacket, ClientError> {
        self.request(ClientPacket::Takeover {
            request_id,
            expected_epoch,
            expires_at_ns,
        })
    }

    pub fn submit_fixture(
        &mut self,
        request_id: u64,
        epoch: u64,
        expires_at_ns: u64,
        fixture_channels: Vec<f32>,
    ) -> Result<ServerPacket, ClientError> {
        self.request(ClientPacket::Target {
            request_id,
            epoch,
            expires_at_ns,
            target: MockTarget { fixture_channels },
        })
    }

    pub fn stop(&mut self, request_id: u64, reason: String) -> Result<ServerPacket, ClientError> {
        self.request(ClientPacket::Stop { request_id, reason })
    }

    pub fn release(
        &mut self,
        request_id: u64,
        expected_epoch: u64,
    ) -> Result<ServerPacket, ClientError> {
        self.request(ClientPacket::Release {
            request_id,
            expected_epoch,
        })
    }

    pub fn clear_fault(
        &mut self,
        request_id: u64,
        expected_epoch: u64,
    ) -> Result<ServerPacket, ClientError> {
        self.request(ClientPacket::ClearFault {
            request_id,
            expected_epoch,
        })
    }

    pub fn result(&mut self, operation_id: u64) -> Result<ServerPacket, ClientError> {
        self.request(ClientPacket::Result { operation_id })
    }

    /// 在调用方原始 monotonic deadline 前，以只读 Result/Status 确认 Acquire 或 Takeover
    /// 已由控制周期应用。
    ///
    /// `expected_applied_epoch` 是动作合同的一部分，必须精确提供：当前 Runtime 的空闲
    /// Acquire 保留请求的 `expected_epoch`；Takeover 只在该值小于 `u64::MAX` 时使用精确的加一结果。`u64::MAX` 没有后继，Runtime 会拒绝新授权，客户端不得用饱和加一把旧完成结果当作新确认。
    /// 每轮最多读取一次 Result，只有完成后才读取一次 Status；轮与轮之间至少等待 1 ms。
    /// 此方法不发送 mutation、不刷新 `expires_at_ns`、不创建 lease，也不会在 `Unknown`
    /// 后重试。`LocalIpcAbortHandle` 仍可从另一线程中止其阻塞的只读读取。
    pub fn await_control_epoch(
        &mut self,
        operation_id: u64,
        expected_applied_epoch: u64,
        expires_at_ns: u64,
    ) -> ControlEpochConfirmation {
        loop {
            let Some(now_ns) = monotonic_now_ns() else {
                return ControlEpochConfirmation::Unknown;
            };
            if now_ns >= expires_at_ns {
                return ControlEpochConfirmation::Unknown;
            }
            match self.result(operation_id) {
                Ok(ServerPacket::Result {
                    operation_id: returned_id,
                    state: WireOperationState::Completed { .. },
                }) if returned_id == operation_id => match self.status() {
                    Ok(ServerPacket::Status { status })
                        if status.control_session_active
                            && status.epoch == expected_applied_epoch =>
                    {
                        return ControlEpochConfirmation::Confirmed {
                            epoch: status.epoch,
                        };
                    }
                    Ok(ServerPacket::Status { .. }) => {}
                    _ => return ControlEpochConfirmation::Unknown,
                },
                Ok(ServerPacket::Result {
                    operation_id: returned_id,
                    state: WireOperationState::Accepted | WireOperationState::Executing,
                }) if returned_id == operation_id => {}
                _ => return ControlEpochConfirmation::Unknown,
            }
            let Some(next_poll_ns) =
                monotonic_now_ns().and_then(|now| now.checked_add(CONTROL_CONFIRMATION_POLL_NS))
            else {
                return ControlEpochConfirmation::Unknown;
            };
            if next_poll_ns >= expires_at_ns || sleep_until_monotonic_ns(next_poll_ns).is_err() {
                return ControlEpochConfirmation::Unknown;
            }
        }
    }
}

impl LocalIpcAbortHandle {
    /// 中止创建该句柄的同一个 `LocalIpcClient` 连接。
    pub fn abort(&self) -> Result<(), ClientError> {
        // client 已析构时 `OwnedFd` 已关闭，Runtime 已可观察 EOF；句柄不延长该连接
        // 生命周期，也不把这一幂等清理误报成新的传输失败。
        match self.fd.upgrade() {
            Some(fd) => shutdown_connection(fd.as_raw_fd()),
            None => Ok(()),
        }
    }
}

fn shutdown_connection(fd: libc::c_int) -> Result<(), ClientError> {
    let result = unsafe { libc::shutdown(fd, libc::SHUT_RDWR) };
    if result < 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ENOTCONN) {
            return Err(ClientError::Io(error));
        }
    }
    Ok(())
}

fn monotonic_now_ns() -> Option<u64> {
    let mut timestamp = unsafe { zeroed::<libc::timespec>() };
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &raw mut timestamp) } < 0 {
        return None;
    }
    let seconds = u64::try_from(timestamp.tv_sec).ok()?;
    let nanoseconds = u64::try_from(timestamp.tv_nsec).ok()?;
    seconds.checked_mul(1_000_000_000)?.checked_add(nanoseconds)
}

fn sleep_until_monotonic_ns(deadline_ns: u64) -> Result<(), ()> {
    let seconds = deadline_ns / 1_000_000_000;
    let nanoseconds = deadline_ns % 1_000_000_000;
    let deadline = libc::timespec {
        tv_sec: seconds.try_into().map_err(|_| ())?,
        tv_nsec: nanoseconds.try_into().map_err(|_| ())?,
    };
    loop {
        let result = unsafe {
            libc::clock_nanosleep(
                libc::CLOCK_MONOTONIC,
                libc::TIMER_ABSTIME,
                &raw const deadline,
                std::ptr::null_mut(),
            )
        };
        if result == 0 {
            return Ok(());
        }
        if result != libc::EINTR {
            return Err(());
        }
    }
}

fn connect_seqpacket(path: &Path, timeout: Duration) -> Result<OwnedFd, ClientError> {
    let bytes = path.as_os_str().as_bytes();
    let max_len = size_of::<libc::sockaddr_un>() - size_of::<libc::sa_family_t>();
    if bytes.is_empty() || bytes.len() + 1 > max_len {
        return Err(ClientError::InvalidConfig(
            "socket_path 不是可编码的 Unix 路径",
        ));
    }
    let _ = CString::new(bytes).map_err(|_| ClientError::InvalidConfig("socket_path 含 NUL"))?;
    let raw = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, 0) };
    if raw < 0 {
        return Err(ClientError::Io(io::Error::last_os_error()));
    }
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    set_timeout(fd.as_raw_fd(), libc::SO_SNDTIMEO, timeout)?;
    set_timeout(fd.as_raw_fd(), libc::SO_RCVTIMEO, timeout)?;
    let mut address = unsafe { zeroed::<libc::sockaddr_un>() };
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    unsafe {
        std::ptr::copy_nonoverlapping(
            bytes.as_ptr().cast(),
            address.sun_path.as_mut_ptr(),
            bytes.len(),
        );
    }
    let result = unsafe {
        libc::connect(
            fd.as_raw_fd(),
            (&raw const address).cast(),
            (size_of::<libc::sa_family_t>() + bytes.len() + 1) as libc::socklen_t,
        )
    };
    if result < 0 {
        return Err(ClientError::Io(io::Error::last_os_error()));
    }
    Ok(fd)
}

fn set_timeout(fd: libc::c_int, option: libc::c_int, timeout: Duration) -> Result<(), ClientError> {
    let value = libc::timeval {
        tv_sec: timeout.as_secs().try_into().unwrap_or(libc::time_t::MAX),
        tv_usec: libc::suseconds_t::from(timeout.subsec_micros()),
    };
    let result = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            option,
            (&raw const value).cast(),
            size_of::<libc::timeval>() as libc::socklen_t,
        )
    };
    if result < 0 {
        return Err(ClientError::Io(io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::fd::AsRawFd,
        sync::{
            Arc, Mutex,
            atomic::{AtomicU64, Ordering},
            mpsc,
        },
        thread,
    };

    static NEXT_SOCKET: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn client_config_rejects_empty_packet_budget() {
        let error = LocalIpcClient::connect(ClientConfig {
            socket_path: "/tmp/no-socket".into(),
            client_label: "test".into(),
            max_packet_bytes: 0,
            io_timeout: Duration::from_millis(1),
        })
        .unwrap_err();
        assert!(matches!(error, ClientError::InvalidConfig(_)));
    }

    #[test]
    fn connect_performs_a_real_seqpacket_hello_and_keeps_runtime_identity() {
        let path = std::env::temp_dir().join(format!(
            "ranger-local-ipc-client-{}-{}.sock",
            std::process::id(),
            NEXT_SOCKET.fetch_add(1, Ordering::Relaxed),
        ));
        let listener = listener(&path);
        let worker = thread::spawn(move || {
            let accepted = unsafe {
                libc::accept4(
                    listener.as_raw_fd(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    libc::SOCK_CLOEXEC,
                )
            };
            assert!(accepted >= 0, "{:?}", io::Error::last_os_error());
            let client = unsafe { OwnedFd::from_raw_fd(accepted) };
            let mut request = [0_u8; 1024];
            let count = unsafe {
                libc::recv(
                    client.as_raw_fd(),
                    request.as_mut_ptr().cast(),
                    request.len(),
                    0,
                )
            };
            assert!(matches!(
                crate::decode_client(&request[..count as usize]).unwrap(),
                ClientPacket::Hello { .. }
            ));
            let response = crate::encode(&ServerPacket::Hello {
                protocol: PROTOCOL.into(),
                clock_domain: CLOCK_DOMAIN.into(),
                boot_id: 41,
                session_id: 7,
                role: crate::LocalRole::Manual,
                clock_mode: crate::ClockMode::Simulated,
                simulated: true,
                real_vehicle_capability: crate::RealVehicleCapability::Unavailable,
            })
            .unwrap();
            let sent = unsafe {
                libc::send(
                    client.as_raw_fd(),
                    response.as_ptr().cast(),
                    response.len(),
                    libc::MSG_NOSIGNAL,
                )
            };
            assert_eq!(sent, response.len() as isize);
        });
        let client = LocalIpcClient::connect(ClientConfig {
            socket_path: path.clone(),
            client_label: "fixture-client".into(),
            max_packet_bytes: 1024,
            io_timeout: Duration::from_millis(100),
        })
        .unwrap();
        assert!(matches!(
            client.hello(),
            ServerPacket::Hello {
                boot_id: 41,
                session_id: 7,
                role: crate::LocalRole::Manual,
                ..
            }
        ));
        worker.join().unwrap();
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn connect_rejects_a_wrong_hello_before_any_mutation() {
        let path = std::env::temp_dir().join(format!(
            "ranger-local-ipc-bad-hello-{}-{}.sock",
            std::process::id(),
            NEXT_SOCKET.fetch_add(1, Ordering::Relaxed),
        ));
        let listener = listener(&path);
        let worker = thread::spawn(move || {
            let accepted = unsafe {
                libc::accept4(
                    listener.as_raw_fd(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    libc::SOCK_CLOEXEC,
                )
            };
            assert!(accepted >= 0, "{:?}", io::Error::last_os_error());
            let client = unsafe { OwnedFd::from_raw_fd(accepted) };
            let mut request = [0_u8; 1024];
            let count = unsafe {
                libc::recv(
                    client.as_raw_fd(),
                    request.as_mut_ptr().cast(),
                    request.len(),
                    0,
                )
            };
            assert!(matches!(
                crate::decode_client(&request[..count as usize]).unwrap(),
                ClientPacket::Hello { .. }
            ));
            let response = crate::encode(&ServerPacket::Hello {
                protocol: "wrong.protocol".into(),
                clock_domain: CLOCK_DOMAIN.into(),
                boot_id: 41,
                session_id: 7,
                role: crate::LocalRole::Manual,
                clock_mode: crate::ClockMode::Simulated,
                simulated: true,
                real_vehicle_capability: crate::RealVehicleCapability::Unavailable,
            })
            .unwrap();
            assert_eq!(
                unsafe {
                    libc::send(
                        client.as_raw_fd(),
                        response.as_ptr().cast(),
                        response.len(),
                        libc::MSG_NOSIGNAL,
                    )
                },
                response.len() as isize
            );
            let next = unsafe {
                libc::recv(
                    client.as_raw_fd(),
                    request.as_mut_ptr().cast(),
                    request.len(),
                    0,
                )
            };
            assert_eq!(next, 0, "错误 Hello 后不能再发送 mutation");
        });
        let error = LocalIpcClient::connect(ClientConfig {
            socket_path: path.clone(),
            client_label: "fixture-client".into(),
            max_packet_bytes: 1024,
            io_timeout: Duration::from_millis(100),
        })
        .unwrap_err();
        assert!(matches!(error, ClientError::Protocol(_)));
        worker.join().unwrap();
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn abort_handle_unblocks_a_request_and_closes_only_its_connection() {
        let path = std::env::temp_dir().join(format!(
            "ranger-local-ipc-abort-{}-{}.sock",
            std::process::id(),
            NEXT_SOCKET.fetch_add(1, Ordering::Relaxed),
        ));
        let listener = listener(&path);
        let (request_seen_tx, request_seen_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let accepted = unsafe {
                libc::accept4(
                    listener.as_raw_fd(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    libc::SOCK_CLOEXEC,
                )
            };
            assert!(accepted >= 0, "{:?}", io::Error::last_os_error());
            let client = unsafe { OwnedFd::from_raw_fd(accepted) };
            let mut packet = [0_u8; 1024];
            let count = unsafe {
                libc::recv(
                    client.as_raw_fd(),
                    packet.as_mut_ptr().cast(),
                    packet.len(),
                    0,
                )
            };
            assert!(matches!(
                crate::decode_client(&packet[..count as usize]).unwrap(),
                ClientPacket::Hello { .. }
            ));
            let hello = crate::encode(&ServerPacket::Hello {
                protocol: PROTOCOL.into(),
                clock_domain: CLOCK_DOMAIN.into(),
                boot_id: 41,
                session_id: 7,
                role: crate::LocalRole::Manual,
                clock_mode: crate::ClockMode::Simulated,
                simulated: true,
                real_vehicle_capability: crate::RealVehicleCapability::Unavailable,
            })
            .unwrap();
            assert_eq!(
                unsafe {
                    libc::send(
                        client.as_raw_fd(),
                        hello.as_ptr().cast(),
                        hello.len(),
                        libc::MSG_NOSIGNAL,
                    )
                },
                hello.len() as isize
            );
            let count = unsafe {
                libc::recv(
                    client.as_raw_fd(),
                    packet.as_mut_ptr().cast(),
                    packet.len(),
                    0,
                )
            };
            assert!(matches!(
                crate::decode_client(&packet[..count as usize]).unwrap(),
                ClientPacket::Status {}
            ));
            request_seen_tx.send(()).unwrap();
            let closed = unsafe {
                libc::recv(
                    client.as_raw_fd(),
                    packet.as_mut_ptr().cast(),
                    packet.len(),
                    0,
                )
            };
            assert_eq!(closed, 0, "abort 必须关闭原有 session");
        });
        let client = LocalIpcClient::connect(ClientConfig {
            socket_path: path.clone(),
            client_label: "fixture-client".into(),
            max_packet_bytes: 1024,
            io_timeout: Duration::from_secs(5),
        })
        .unwrap();
        let abort = client.abort_handle();
        let client = Arc::new(Mutex::new(client));
        let blocked = {
            let client = Arc::clone(&client);
            thread::spawn(move || client.lock().unwrap().status())
        };
        request_seen_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        abort.abort().unwrap();
        assert!(matches!(blocked.join().unwrap(), Err(ClientError::Io(_))));
        worker.join().unwrap();
        drop(client);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn abort_handle_does_not_keep_a_dropped_client_session_alive() {
        let path = std::env::temp_dir().join(format!(
            "ranger-local-ipc-abort-drop-{}-{}.sock",
            std::process::id(),
            NEXT_SOCKET.fetch_add(1, Ordering::Relaxed),
        ));
        let listener = listener(&path);
        let worker = thread::spawn(move || {
            let accepted = unsafe {
                libc::accept4(
                    listener.as_raw_fd(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    libc::SOCK_CLOEXEC,
                )
            };
            assert!(accepted >= 0, "{:?}", io::Error::last_os_error());
            let client = unsafe { OwnedFd::from_raw_fd(accepted) };
            let mut packet = [0_u8; 1024];
            let count = unsafe {
                libc::recv(
                    client.as_raw_fd(),
                    packet.as_mut_ptr().cast(),
                    packet.len(),
                    0,
                )
            };
            assert!(matches!(
                crate::decode_client(&packet[..count as usize]).unwrap(),
                ClientPacket::Hello { .. }
            ));
            let hello = crate::encode(&ServerPacket::Hello {
                protocol: PROTOCOL.into(),
                clock_domain: CLOCK_DOMAIN.into(),
                boot_id: 41,
                session_id: 7,
                role: crate::LocalRole::Manual,
                clock_mode: crate::ClockMode::Simulated,
                simulated: true,
                real_vehicle_capability: crate::RealVehicleCapability::Unavailable,
            })
            .unwrap();
            assert_eq!(
                unsafe {
                    libc::send(
                        client.as_raw_fd(),
                        hello.as_ptr().cast(),
                        hello.len(),
                        libc::MSG_NOSIGNAL,
                    )
                },
                hello.len() as isize
            );
            let closed = unsafe {
                libc::recv(
                    client.as_raw_fd(),
                    packet.as_mut_ptr().cast(),
                    packet.len(),
                    0,
                )
            };
            assert_eq!(closed, 0, "drop client 必须关闭原有 session");
        });
        let client = LocalIpcClient::connect(ClientConfig {
            socket_path: path.clone(),
            client_label: "fixture-client".into(),
            max_packet_bytes: 1024,
            io_timeout: Duration::from_secs(1),
        })
        .unwrap();
        let abort = client.abort_handle();
        drop(client);
        assert!(abort.abort().is_ok(), "失效句柄应保持幂等清理语义");
        worker.join().unwrap();
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn control_epoch_confirmation_waits_for_a_delayed_cycle_application() {
        let path = std::env::temp_dir().join(format!(
            "ranger-local-ipc-confirm-{}-{}.sock",
            std::process::id(),
            NEXT_SOCKET.fetch_add(1, Ordering::Relaxed),
        ));
        let listener = listener(&path);
        let worker = thread::spawn(move || {
            let accepted = unsafe {
                libc::accept4(
                    listener.as_raw_fd(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    libc::SOCK_CLOEXEC,
                )
            };
            assert!(accepted >= 0, "{:?}", io::Error::last_os_error());
            let client = unsafe { OwnedFd::from_raw_fd(accepted) };
            let mut packet = [0_u8; 1024];
            assert!(matches!(
                receive_packet(&client, &mut packet),
                ClientPacket::Hello { .. }
            ));
            send_packet(
                &client,
                ServerPacket::Hello {
                    protocol: PROTOCOL.into(),
                    clock_domain: CLOCK_DOMAIN.into(),
                    boot_id: 41,
                    session_id: 7,
                    role: crate::LocalRole::Manual,
                    clock_mode: crate::ClockMode::Simulated,
                    simulated: true,
                    real_vehicle_capability: crate::RealVehicleCapability::Unavailable,
                },
            );
            assert!(matches!(
                receive_packet(&client, &mut packet),
                ClientPacket::Result { operation_id: 11 }
            ));
            send_packet(
                &client,
                ServerPacket::Result {
                    operation_id: 11,
                    state: WireOperationState::Accepted,
                },
            );
            assert!(matches!(
                receive_packet(&client, &mut packet),
                ClientPacket::Result { operation_id: 11 }
            ));
            send_packet(
                &client,
                ServerPacket::Result {
                    operation_id: 11,
                    state: WireOperationState::Executing,
                },
            );
            assert!(matches!(
                receive_packet(&client, &mut packet),
                ClientPacket::Result { operation_id: 11 }
            ));
            send_packet(
                &client,
                ServerPacket::Result {
                    operation_id: 11,
                    state: WireOperationState::Completed {
                        evidence: crate::ExecutionEvidence::HostApplied,
                    },
                },
            );
            assert!(matches!(
                receive_packet(&client, &mut packet),
                ClientPacket::Status {}
            ));
            send_packet(&client, status_packet(false, 5));
            assert!(matches!(
                receive_packet(&client, &mut packet),
                ClientPacket::Result { operation_id: 11 }
            ));
            send_packet(
                &client,
                ServerPacket::Result {
                    operation_id: 11,
                    state: WireOperationState::Completed {
                        evidence: crate::ExecutionEvidence::HostApplied,
                    },
                },
            );
            assert!(matches!(
                receive_packet(&client, &mut packet),
                ClientPacket::Status {}
            ));
            send_packet(&client, status_packet(true, 6));
        });
        let mut client = LocalIpcClient::connect(ClientConfig {
            socket_path: path.clone(),
            client_label: "fixture-client".into(),
            max_packet_bytes: 1024,
            io_timeout: Duration::from_millis(100),
        })
        .unwrap();
        let expires_at_ns = monotonic_now_ns().unwrap() + 100_000_000;
        assert_eq!(
            client.await_control_epoch(11, 6, expires_at_ns),
            ControlEpochConfirmation::Confirmed { epoch: 6 }
        );
        worker.join().unwrap();
        std::fs::remove_file(path).unwrap();
    }

    fn receive_packet(stream: &OwnedFd, packet: &mut [u8]) -> ClientPacket {
        let count = unsafe {
            libc::recv(
                stream.as_raw_fd(),
                packet.as_mut_ptr().cast(),
                packet.len(),
                0,
            )
        };
        assert!(count > 0, "{:?}", io::Error::last_os_error());
        crate::decode_client(&packet[..count as usize]).unwrap()
    }

    fn send_packet(stream: &OwnedFd, packet: ServerPacket) {
        let bytes = crate::encode(&packet).unwrap();
        assert_eq!(
            unsafe {
                libc::send(
                    stream.as_raw_fd(),
                    bytes.as_ptr().cast(),
                    bytes.len(),
                    libc::MSG_NOSIGNAL,
                )
            },
            bytes.len() as isize
        );
    }

    fn status_packet(active: bool, epoch: u64) -> ServerPacket {
        ServerPacket::Status {
            status: crate::MockStatus {
                simulated: true,
                clock_mode: crate::ClockMode::Simulated,
                real_vehicle_capability: crate::RealVehicleCapability::Unavailable,
                control_session_active: active,
                fault_latched: false,
                epoch,
                cycle_id: Some(1),
                sources: std::array::from_fn(|channel_index| crate::MockSource {
                    channel_index: channel_index as u8,
                    observed_at_ns: 1,
                    quality: crate::FixtureQuality::Usable,
                    simulated: true,
                }),
            },
        }
    }

    fn listener(path: &Path) -> OwnedFd {
        let raw =
            unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, 0) };
        assert!(raw >= 0, "{:?}", io::Error::last_os_error());
        let listener = unsafe { OwnedFd::from_raw_fd(raw) };
        let bytes = path.as_os_str().as_bytes();
        let mut address = unsafe { zeroed::<libc::sockaddr_un>() };
        address.sun_family = libc::AF_UNIX as libc::sa_family_t;
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr().cast(),
                address.sun_path.as_mut_ptr(),
                bytes.len(),
            );
        }
        let bound = unsafe {
            libc::bind(
                listener.as_raw_fd(),
                (&raw const address).cast(),
                (size_of::<libc::sa_family_t>() + bytes.len() + 1) as libc::socklen_t,
            )
        };
        assert_eq!(bound, 0, "{:?}", io::Error::last_os_error());
        assert_eq!(unsafe { libc::listen(listener.as_raw_fd(), 1) }, 0);
        listener
    }
}
