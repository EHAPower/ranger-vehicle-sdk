//! Ranger Vehicle Runtime 的最小同步 Rust Client SDK。
//!
//! 本 crate 只封装 Linux Unix domain `SOCK_SEQPACKET` transport、control session
//! 和车辆级 motion/stop 请求，不包含硬件访问、安全策略或自动重连。

use std::os::fd::{AsRawFd, OwnedFd};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use contracts::generated::command::{
    ClockDomain, CommandTarget, ControlFrame, EhaFaultResetCommand, FrameHeader, Resource,
    StopCommand, UnitSpec, VehicleCommand,
};
pub use contracts::generated::receipt::{GateReceipt, ReceiptStatus};
use contracts::generated::session::{
    Heartbeat, HeartbeatAck, SessionHandshake, SessionResponse, SessionStatus,
};
use contracts::generated::telemetry::{ServerError, ServerErrorCodeRepr};
use contracts::transport::bootstrap::{
    ConnectRequest, ConnectResponse, ConnectStatusRepr, SessionTypeRepr,
};
use contracts::transport::envelope::{Envelope, EnvelopeError, HEADER_LEN, MsgKind};
use nix::errno::Errno;
use nix::sys::socket::{
    AddressFamily, MsgFlags, SockFlag, SockType, UnixAddr, connect, recv, send, socket,
};
use nix::time::{ClockId, clock_gettime};
use thiserror::Error;

/// SDK transport 配置。所有字段必须由调用方显式提供。
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub socket_path: PathBuf,
    pub max_frame_bytes: u32,
}

/// 车辆级连续运动目标。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VehicleMotionCommand {
    pub linear_velocity_mps: f64,
    pub yaw_rate_radps: f64,
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("ClientConfig 无效：{0}")]
    InvalidConfig(&'static str),

    #[error("系统调用失败：{0}")]
    System(#[from] Errno),

    #[error("wire envelope 错误：{0}")]
    Envelope(#[from] EnvelopeError),

    #[error("协议错误：{0}")]
    Protocol(String),

    #[error("期望 {expected}，收到 {actual}")]
    UnexpectedMessage { expected: MsgKind, actual: MsgKind },

    #[error("Server 拒绝连接：{0:?}")]
    ConnectRejected(ConnectStatusRepr),

    #[error("Server 未授予 Control session 权限")]
    ControlSessionNotAllowed,

    #[error("Server 拒绝 Control session：{0:?}")]
    SessionRejected(SessionStatus),

    #[error("Server handshake 无效：{0}")]
    InvalidHandshake(&'static str),

    #[error("ServerError {code:?}: {message}")]
    Server {
        code: ServerErrorCodeRepr,
        message: String,
    },

    #[error("发送长度不完整：期望 {expected} 字节，实际 {actual} 字节")]
    ShortSend { expected: usize, actual: usize },

    #[error("Server 已关闭连接")]
    PeerClosed,

    #[error("request/sequence 计数器耗尽")]
    CounterExhausted,

    #[error("Server receipt request_id 不匹配：期望 {expected}，实际 {actual}")]
    RequestIdMismatch { expected: u64, actual: u64 },

    #[error("VehicleMotionCommand 字段必须是有限数")]
    NonFiniteMotionCommand,

    #[error("profile_id 不能为空")]
    EmptyProfileId,

    #[error("stop reason 不能为空")]
    EmptyStopReason,

    #[error("EHA 故障复位 reason 不能为空")]
    EmptyEhaFaultResetReason,

    #[error("CLOCK_MONOTONIC 时间超出 u64 纳秒范围")]
    ClockOutOfRange,
}

pub type Result<T> = std::result::Result<T, ClientError>;

/// 唯一 active control session 的同步 client。
pub struct ControlClient {
    stream: OwnedFd,
    max_frame_bytes: u32,
    session_id: u64,
    heartbeat_interval: Duration,
    command_default_valid_for_ms: u64,
    last_heartbeat_at: Instant,
    next_request_id: u64,
    next_sequence: u64,
}

impl ControlClient {
    /// 连接 Ranger Vehicle Runtime，完成 bootstrap 和 Control session handshake。
    pub fn connect(config: ClientConfig) -> Result<Self> {
        validate_config(&config)?;
        let stream = socket(
            AddressFamily::Unix,
            SockType::SeqPacket,
            SockFlag::SOCK_CLOEXEC,
            None,
        )?;
        let address = UnixAddr::new(&config.socket_path)?;
        connect(stream.as_raw_fd(), &address)?;
        Self::from_connected_fd(stream, config.max_frame_bytes)
    }

    /// 发送一个车辆级 motion frame，并返回 Server gate receipt。
    pub fn send_motion(
        &mut self,
        profile_id: &str,
        command: VehicleMotionCommand,
    ) -> Result<GateReceipt> {
        if profile_id.is_empty() {
            return Err(ClientError::EmptyProfileId);
        }
        if !command.linear_velocity_mps.is_finite() || !command.yaw_rate_radps.is_finite() {
            return Err(ClientError::NonFiniteMotionCommand);
        }
        self.heartbeat_if_due()?;

        let (request_id, sequence) = self.take_request_and_sequence()?;
        let frame = ControlFrame {
            header: self.frame_header(request_id)?,
            sequence,
            valid_for_ms: self.command_default_valid_for_ms,
            profile_id: profile_id.to_owned(),
            commands: vec![VehicleCommand {
                command_id: format!("motion-{request_id}"),
                resource: Resource::VehicleMotion,
                target: CommandTarget {
                    fields: vec![
                        (
                            "linear_velocity_mps".to_owned(),
                            command.linear_velocity_mps,
                        ),
                        ("yaw_rate_radps".to_owned(), command.yaw_rate_radps),
                    ],
                },
                units: vec![
                    UnitSpec {
                        field: "linear_velocity_mps".to_owned(),
                        unit: "m/s".to_owned(),
                    },
                    UnitSpec {
                        field: "yaw_rate_radps".to_owned(),
                        unit: "rad/s".to_owned(),
                    },
                ],
            }],
        };
        let envelope = Envelope::with_bincode_payload(MsgKind::ControlFrame, &frame)?;
        self.send_envelope(&envelope)?;
        self.receive_receipt(request_id)
    }

    /// 显式发送 session heartbeat。
    pub fn heartbeat(&mut self) -> Result<()> {
        let heartbeat = Heartbeat {
            timestamp_ns: monotonic_ns()?,
        };
        let envelope = Envelope::with_bincode_payload(MsgKind::Heartbeat, &heartbeat)?;
        self.send_envelope(&envelope)?;
        let response = self.receive_expected(MsgKind::HeartbeatAck)?;
        let _: HeartbeatAck = contracts::transport::codec::deserialize(
            &response.payload,
            u64::from(self.max_frame_bytes),
        )
        .map_err(|error| ClientError::Protocol(format!("HeartbeatAck 解码失败：{error}")))?;
        self.last_heartbeat_at = Instant::now();
        Ok(())
    }

    /// 请求车辆进入更安全的停止状态，并返回同步 gate receipt。
    pub fn stop(&mut self, reason: &str) -> Result<GateReceipt> {
        if reason.is_empty() {
            return Err(ClientError::EmptyStopReason);
        }
        self.heartbeat_if_due()?;
        let request_id = self.take_request_id()?;
        let command = StopCommand {
            header: self.frame_header(request_id)?,
            reason_code: reason.to_owned(),
        };
        let envelope = Envelope::with_bincode_payload(MsgKind::StopCommand, &command)?;
        self.send_envelope(&envelope)?;
        self.receive_receipt(request_id)
    }

    /// 请求复位 EHA 域故障锁存（`eha_domain_fault`，isolated 款式），并返回
    /// 同步 gate receipt。
    ///
    /// receipt Accepted 只表示复位请求已被接受：实际清除发生在 Server 的
    /// rt_control 下一周期，故障原因未消除时会被重新锁存；调用方应通过
    /// `/safety/eha_domain_fault` 诊断确认最终状态。全局 E-Stop 激活期间
    /// 请求会被拒绝（`estop_active`）。
    pub fn reset_eha_fault(&mut self, reason: &str) -> Result<GateReceipt> {
        if reason.is_empty() {
            return Err(ClientError::EmptyEhaFaultResetReason);
        }
        self.heartbeat_if_due()?;
        let request_id = self.take_request_id()?;
        let command = EhaFaultResetCommand {
            header: self.frame_header(request_id)?,
            reason_code: reason.to_owned(),
        };
        let envelope = Envelope::with_bincode_payload(MsgKind::EhaFaultResetCommand, &command)?;
        self.send_envelope(&envelope)?;
        self.receive_receipt(request_id)
    }

    fn heartbeat_if_due(&mut self) -> Result<()> {
        if self.last_heartbeat_at.elapsed() >= self.heartbeat_interval {
            self.heartbeat()?;
        }
        Ok(())
    }

    fn from_connected_fd(stream: OwnedFd, max_frame_bytes: u32) -> Result<Self> {
        if max_frame_bytes == 0 {
            return Err(ClientError::InvalidConfig("max_frame_bytes 必须大于 0"));
        }
        let mut client = Self {
            stream,
            max_frame_bytes,
            session_id: 0,
            heartbeat_interval: Duration::ZERO,
            command_default_valid_for_ms: 0,
            last_heartbeat_at: Instant::now(),
            next_request_id: 1,
            next_sequence: 1,
        };

        let connect_request = ConnectRequest {
            sdk_version: env!("CARGO_PKG_VERSION").to_owned(),
            // 当前 Server 尚未返回 schema negotiation；见 sdk/README.md 的兼容限制。
            schemas: Vec::new(),
        };
        let envelope = connect_request
            .encode_envelope()
            .map_err(ClientError::Protocol)?;
        client.send_envelope(&envelope)?;
        let envelope = client.receive_expected(MsgKind::ConnectResponse)?;
        let connect_response: ConnectResponse =
            contracts::transport::codec::deserialize(&envelope.payload, u64::from(max_frame_bytes))
                .map_err(|error| {
                    ClientError::Protocol(format!("ConnectResponse 解码失败：{error}"))
                })?;
        if connect_response.status != ConnectStatusRepr::Ok {
            return Err(ClientError::ConnectRejected(connect_response.status));
        }
        if !connect_response
            .allowed_session_types
            .contains(&SessionTypeRepr::Control)
        {
            return Err(ClientError::ControlSessionNotAllowed);
        }

        let handshake = SessionHandshake {
            session_type: SessionTypeRepr::Control,
        };
        client.send_envelope(&Envelope::with_bincode_payload(
            MsgKind::SessionHandshake,
            &handshake,
        )?)?;
        let envelope = client.receive_expected(MsgKind::SessionResponse)?;
        let session: SessionResponse =
            contracts::transport::codec::deserialize(&envelope.payload, u64::from(max_frame_bytes))
                .map_err(|error| {
                    ClientError::Protocol(format!("SessionResponse 解码失败：{error}"))
                })?;
        if session.status != SessionStatus::Ok {
            return Err(ClientError::SessionRejected(session.status));
        }
        validate_session_response(&session)?;

        client.session_id = session.session_id;
        client.heartbeat_interval = Duration::from_millis(session.heartbeat_interval_ms);
        client.command_default_valid_for_ms = session.command_default_valid_for_ms;
        client.last_heartbeat_at = Instant::now();
        Ok(client)
    }

    fn frame_header(&self, request_id: u64) -> Result<FrameHeader> {
        Ok(FrameHeader {
            request_id,
            session_id: self.session_id,
            source_timestamp_ns: monotonic_ns()?,
            source_clock: ClockDomain::Monotonic,
        })
    }

    fn take_request_and_sequence(&mut self) -> Result<(u64, u64)> {
        let next_request_id = self
            .next_request_id
            .checked_add(1)
            .ok_or(ClientError::CounterExhausted)?;
        let next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(ClientError::CounterExhausted)?;
        let current = (self.next_request_id, self.next_sequence);
        self.next_request_id = next_request_id;
        self.next_sequence = next_sequence;
        Ok(current)
    }

    fn take_request_id(&mut self) -> Result<u64> {
        let next = self
            .next_request_id
            .checked_add(1)
            .ok_or(ClientError::CounterExhausted)?;
        let current = self.next_request_id;
        self.next_request_id = next;
        Ok(current)
    }

    fn receive_receipt(&self, request_id: u64) -> Result<GateReceipt> {
        let envelope = self.receive_expected(MsgKind::GateReceipt)?;
        let receipt: GateReceipt = contracts::transport::codec::deserialize(
            &envelope.payload,
            u64::from(self.max_frame_bytes),
        )
        .map_err(|error| ClientError::Protocol(format!("GateReceipt 解码失败：{error}")))?;
        if receipt.request_id != request_id {
            return Err(ClientError::RequestIdMismatch {
                expected: request_id,
                actual: receipt.request_id,
            });
        }
        Ok(receipt)
    }

    fn send_envelope(&self, envelope: &Envelope) -> Result<()> {
        if envelope.payload.len() > self.max_frame_bytes as usize {
            return Err(ClientError::Envelope(EnvelopeError::FrameTooLarge {
                actual: u32::try_from(envelope.payload.len()).unwrap_or(u32::MAX),
                max: self.max_frame_bytes,
            }));
        }
        let bytes = envelope.encode();
        let sent = send(self.stream.as_raw_fd(), &bytes, MsgFlags::empty())?;
        if sent != bytes.len() {
            return Err(ClientError::ShortSend {
                expected: bytes.len(),
                actual: sent,
            });
        }
        Ok(())
    }

    fn receive_expected(&self, expected: MsgKind) -> Result<Envelope> {
        let envelope = self.receive_envelope()?;
        if envelope.msg_kind == MsgKind::ServerError {
            let error: ServerError = contracts::transport::codec::deserialize(
                &envelope.payload,
                u64::from(self.max_frame_bytes),
            )
            .map_err(|error| ClientError::Protocol(format!("ServerError 解码失败：{error}")))?;
            return Err(ClientError::Server {
                code: error.code,
                message: error.message,
            });
        }
        if envelope.msg_kind != expected {
            return Err(ClientError::UnexpectedMessage {
                expected,
                actual: envelope.msg_kind,
            });
        }
        Ok(envelope)
    }

    fn receive_envelope(&self) -> Result<Envelope> {
        let capacity = HEADER_LEN
            .checked_add(self.max_frame_bytes as usize)
            .ok_or(ClientError::InvalidConfig("max_frame_bytes 过大"))?;
        let mut buffer = vec![0_u8; capacity];
        let received = recv(self.stream.as_raw_fd(), &mut buffer, MsgFlags::MSG_TRUNC)?;
        if received == 0 {
            return Err(ClientError::PeerClosed);
        }
        if received > buffer.len() {
            return Err(ClientError::Envelope(EnvelopeError::FrameTruncated));
        }
        Ok(Envelope::decode(&buffer[..received], self.max_frame_bytes)?)
    }
}

fn validate_config(config: &ClientConfig) -> Result<()> {
    if config.socket_path.as_os_str().is_empty() {
        return Err(ClientError::InvalidConfig("socket_path 不能为空"));
    }
    if config.max_frame_bytes == 0 {
        return Err(ClientError::InvalidConfig("max_frame_bytes 必须大于 0"));
    }
    Ok(())
}

fn validate_session_response(session: &SessionResponse) -> Result<()> {
    if session.session_id == 0 {
        return Err(ClientError::InvalidHandshake("session_id 不能为 0"));
    }
    if session.heartbeat_interval_ms == 0
        || session.heartbeat_timeout_ms <= session.heartbeat_interval_ms
    {
        return Err(ClientError::InvalidHandshake(
            "heartbeat timeout 必须大于非零 interval",
        ));
    }
    if session.command_default_valid_for_ms == 0
        || session.command_max_valid_for_ms <= session.command_default_valid_for_ms
    {
        return Err(ClientError::InvalidHandshake(
            "command max valid time 必须大于非零 default valid time",
        ));
    }
    Ok(())
}

fn monotonic_ns() -> Result<u64> {
    let now = clock_gettime(ClockId::CLOCK_MONOTONIC)?;
    let seconds = u64::try_from(now.tv_sec()).map_err(|_| ClientError::ClockOutOfRange)?;
    let nanoseconds = u64::try_from(now.tv_nsec()).map_err(|_| ClientError::ClockOutOfRange)?;
    seconds
        .checked_mul(1_000_000_000)
        .and_then(|value| value.checked_add(nanoseconds))
        .ok_or(ClientError::ClockOutOfRange)
}

#[cfg(test)]
mod tests;
