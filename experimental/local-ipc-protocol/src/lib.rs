//! Ranger mock 软件运行时的实验性 Unix `SOCK_SEQPACKET` JSON 合同。
//!
//! 本合同与旧 SDK snapshot 的 RNG1/bincode 协议没有兼容关系。每个 packet 是一条完整
//! JSON 消息，Server 在读 socket 前后都施加容量与身份限制。

use serde::{Deserialize, Serialize};

/// 实验性协议的唯一标识。
pub const PROTOCOL: &str = "ranger.mock-runtime.seqpacket.v1";

/// 客户端与 Runtime 共用的单调时钟域名称。
pub const CLOCK_DOMAIN: &str = "linux.boot_monotonic";

/// `Hello` 允许的 SDK 标签最大 UTF-8 字节数。
pub const MAX_CLIENT_LABEL_BYTES: usize = 64;

/// 停止原因最大 UTF-8 字节数。
pub const MAX_STOP_REASON_BYTES: usize = 96;

/// 客户端发给 Server 的一个完整 JSON packet。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientPacket {
    /// 必须是连接的第一条消息，确认实验协议、mock 意图与时钟域。
    Hello {
        protocol: String,
        clock_domain: String,
        client_label: String,
    },
    /// 申请一个控制会话。
    Acquire {
        request_id: u64,
        expected_epoch: u64,
        expires_at_ns: u64,
    },
    /// 提交 fixture 目标。`expires_at_ns` 为原始绝对 monotonic 截止期。
    Target {
        request_id: u64,
        /// 客户端观察到的控制代次；Server 原样交给 Runtime，不自行补写。
        epoch: u64,
        expires_at_ns: u64,
        target: MockTarget,
    },
    /// 请求停止。它在 Server 内走独立的 Runtime 停止路径。
    Stop { request_id: u64, reason: String },
    /// 请求清除可清除的 mock 软件故障；不会恢复会话或旧目标。
    ClearFault {
        request_id: u64,
        expected_epoch: u64,
    },
    /// 读取当前投影状态。
    Status {},
    /// 查询本会话拥有的操作终态。
    Result { operation_id: u64 },
}

/// Server 发给客户端的一个完整 JSON packet。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServerPacket {
    /// Server 确认协议、会话身份和明确的 mock 能力边界。
    Hello {
        protocol: String,
        clock_domain: String,
        boot_id: u64,
        session_id: u64,
        simulated: bool,
        real_vehicle_capability: RealVehicleCapability,
    },
    /// 请求的同步准入结论。
    Admission {
        request_id: u64,
        decision: AdmissionDecision,
    },
    /// 当前 mock Runtime 状态投影。
    Status { status: MockStatus },
    /// 一个有 owner 的操作状态或终态。
    Result {
        operation_id: u64,
        state: OperationState,
    },
    /// packet 或当前会话无效；不会暗示请求未产生副作用。
    Error { code: ErrorCode, detail: String },
}

/// 固定数量的 mock fixture 通道值。
///
/// 它不对应速度、转向、设备电流或任何车辆运动学量。通道数由 Server/Runtime 的 fixture
/// 配置验证，协议层不猜测真实设备布局。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MockTarget {
    pub fixture_channels: Vec<f32>,
}

/// 请求接纳结论。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum AdmissionDecision {
    Accepted { operation_id: u64 },
    Rejected { reason: RejectReason },
}

/// Server 可稳定表达的准入拒绝原因。
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectReason {
    HandshakeRequired,
    DuplicateRequestId,
    SessionUnavailable,
    SessionNotOwner,
    TargetExpired,
    TargetInvalid,
    RuntimeUnavailable,
    ResultCapacityExhausted,
    FaultLatched,
}

/// 有身份操作的当前或最终状态。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum OperationState {
    Accepted,
    Executing,
    Completed {
        evidence: ExecutionEvidence,
    },
    Failed {
        reason: String,
        evidence: ExecutionEvidence,
    },
    ResultUnknown,
}

/// mock Runtime 的只读状态投影。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MockStatus {
    pub simulated: bool,
    pub real_vehicle_capability: RealVehicleCapability,
    pub control_session_active: bool,
    pub fault_latched: bool,
    /// 当前控制代次；客户端必须把它原样写入后续 `Target`。
    pub epoch: u64,
    /// mock 控制循环的最近周期号；尚未运行时为 `None`，不是设备执行证据。
    pub cycle_id: Option<u64>,
    /// 八个固定 fixture 通道的来源时间和质量；它不是车辆状态估计或真实设备遥测。
    pub sources: [MockSource; 8],
}

/// 一个固定 mock 通道的观测投影。
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MockSource {
    pub channel_index: u8,
    pub observed_at_ns: u64,
    pub quality: FixtureQuality,
    /// 始终为 true；该记录不是物理设备观测。
    pub simulated: bool,
}

/// fixture 通道数据的来源和质量。
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureQuality {
    Usable,
    Stale,
    Invalid,
}

/// mock 操作终态的唯一证据类型。
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionEvidence {
    SimulatedOnly,
    /// Runtime 已应用控制权或处理请求；没有设备执行或停止确认。
    HostApplied,
    NoExecutionEvidence,
}

/// 真实车辆能力的显式边界。
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RealVehicleCapability {
    Unavailable,
}

/// 协议级错误代码。
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    ProtocolMismatch,
    ClockDomainMismatch,
    InvalidMessage,
    NotAuthorized,
    NotFound,
    Overloaded,
}

/// 将 packet 编码为一个 JSON `SOCK_SEQPACKET` payload。
pub fn encode<T: Serialize>(packet: &T) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(packet)
}

/// 从一个完整 `SOCK_SEQPACKET` payload 解码 JSON。
pub fn decode_client(packet: &[u8]) -> Result<ClientPacket, serde_json::Error> {
    serde_json::from_slice(packet)
}

/// 验证握手中的固定合同声明和可控字符串长度。
pub fn validate_hello(protocol: &str, clock_domain: &str, client_label: &str) -> bool {
    protocol == PROTOCOL
        && clock_domain == CLOCK_DOMAIN
        && !client_label.is_empty()
        && client_label.len() <= MAX_CLIENT_LABEL_BYTES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_packet_roundtrip_keeps_absolute_expiry() {
        let packet = ClientPacket::Target {
            request_id: 7,
            epoch: 3,
            expires_at_ns: 123_456,
            target: MockTarget {
                fixture_channels: vec![0.0, -0.5],
            },
        };
        assert_eq!(decode_client(&encode(&packet).unwrap()).unwrap(), packet);
    }

    #[test]
    fn hello_requires_the_mock_protocol_and_monotonic_clock() {
        assert!(validate_hello(PROTOCOL, CLOCK_DOMAIN, "fixture-client"));
        assert!(!validate_hello("RNG1", CLOCK_DOMAIN, "fixture-client"));
        assert!(!validate_hello(PROTOCOL, "epoch", "fixture-client"));
    }

    #[test]
    fn unknown_json_field_is_not_silently_accepted() {
        assert!(decode_client(br#"{"type":"status","old_rng1_field":true}"#).is_err());
    }
}
