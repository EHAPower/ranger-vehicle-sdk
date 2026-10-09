//! v1 ControlFrame 和 StopCommand DTO。
//!
//! 对应 `sdk/schema/command_request.yaml` 的 `ControlFrame` 和 `OperationRequest(stop)`。

use serde::{Deserialize, Serialize};

/// 时钟域。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClockDomain {
    Monotonic = 0,
    Epoch = 1,
    Unspecified = 2,
}

/// 帧级 header，附在每个 ControlFrame / StopCommand 的 payload 内。
///
/// 注意：wp_ver 和 msg_kind 已在 envelope 中携带，此处不重复。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameHeader {
    pub request_id: u64,
    pub session_id: u64,
    pub source_timestamp_ns: u64,
    pub source_clock: ClockDomain,
}

/// 车辆级控制命令帧。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlFrame {
    pub header: FrameHeader,
    pub sequence: u64,
    pub valid_for_ms: u64,
    pub profile_id: String,
    pub commands: Vec<VehicleCommand>,
}

/// 帧内单个车辆级命令。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleCommand {
    pub command_id: String,
    pub resource: Resource,
    pub target: CommandTarget,
    pub units: Vec<UnitSpec>,
}

/// 资源枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Resource {
    #[serde(rename = "vehicle.motion")]
    VehicleMotion,
    #[serde(rename = "vehicle.suspension")]
    VehicleSuspension,
    #[serde(rename = "vehicle.corner.front_left")]
    CornerFrontLeft,
    #[serde(rename = "vehicle.corner.front_right")]
    CornerFrontRight,
    #[serde(rename = "vehicle.corner.rear_left")]
    CornerRearLeft,
    #[serde(rename = "vehicle.corner.rear_right")]
    CornerRearRight,
}

/// 命令目标。v1 使用自由字段；gate 负责校验。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandTarget {
    pub fields: Vec<(String, f64)>,
}

/// 单位标注。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitSpec {
    pub field: String,
    pub unit: String,
}

/// 停车/禁用命令。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopCommand {
    pub header: FrameHeader,
    pub reason_code: String,
}

/// EHA 域故障锁存显式复位命令（isolated 款式 `eha_domain_fault`）。
///
/// 仅清除 EHA 域锁存，不触碰全局 E-Stop、Supervisor 或 command epoch；
/// 复位后故障原因仍未消除时由 rt_control 在同一周期内重新锁存。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EhaFaultResetCommand {
    pub header: FrameHeader,
    pub reason_code: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_frame_roundtrip() {
        let frame = ControlFrame {
            header: FrameHeader {
                request_id: 42,
                session_id: 1,
                source_timestamp_ns: 1_000_000_000,
                source_clock: ClockDomain::Monotonic,
            },
            sequence: 7,
            valid_for_ms: 200,
            profile_id: "yard-low-speed.v1".to_string(),
            commands: vec![VehicleCommand {
                command_id: "cmd-1".to_string(),
                resource: Resource::VehicleMotion,
                target: CommandTarget {
                    fields: vec![
                        ("linear_velocity_mps".to_string(), 0.8),
                        ("yaw_rate_radps".to_string(), 0.15),
                    ],
                },
                units: vec![
                    UnitSpec {
                        field: "linear_velocity".to_string(),
                        unit: "m/s".to_string(),
                    },
                    UnitSpec {
                        field: "yaw_rate".to_string(),
                        unit: "rad/s".to_string(),
                    },
                ],
            }],
        };

        let bytes = bincode::serialize(&frame).unwrap();
        let decoded: ControlFrame = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded.sequence, 7);
        assert_eq!(decoded.commands.len(), 1);
        assert_eq!(decoded.header.source_clock, ClockDomain::Monotonic);
    }

    #[test]
    fn stop_command_roundtrip() {
        let stop = StopCommand {
            header: FrameHeader {
                request_id: 100,
                session_id: 1,
                source_timestamp_ns: 2_000_000_000,
                source_clock: ClockDomain::Monotonic,
            },
            reason_code: "operator_stop".to_string(),
        };
        let bytes = bincode::serialize(&stop).unwrap();
        let decoded: StopCommand = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded.reason_code, "operator_stop");
    }

    #[test]
    fn eha_fault_reset_command_roundtrip() {
        let reset = EhaFaultResetCommand {
            header: FrameHeader {
                request_id: 101,
                session_id: 1,
                source_timestamp_ns: 3_000_000_000,
                source_clock: ClockDomain::Monotonic,
            },
            reason_code: "operator_acknowledge".to_string(),
        };
        let bytes = bincode::serialize(&reset).unwrap();
        let decoded: EhaFaultResetCommand = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded.reason_code, "operator_acknowledge");
        assert_eq!(decoded.header.request_id, 101);
    }
}
