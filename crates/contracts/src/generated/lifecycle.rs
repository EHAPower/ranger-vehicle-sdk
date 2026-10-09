//! Ranger Server API v2 车辆生命周期 DTO。
//!
//! 这些类型的 transport `MsgKind` 已固定在 WP_VER=2，并由 Server API 适配
//! `vehicle_core::safety` 的 mode / safety / operation 语义。它们只表达 host
//! 协议和证据；不能用来宣称车辆已经支持 active mode 或 backend stop confirmation。

use serde::{Deserialize, Serialize};

use super::command::FrameHeader;

/// v2 lifecycle wire 的稳定 reason code。
///
/// `vehicle_core` 的紧凑 enum 由 Server adapter 显式映射到这些字符串；core
/// 不依赖 contracts，backend-private 诊断不得冒充这里的 motion 准入原因。
pub mod reason {
    pub const ACCEPTED: &str = "accepted";
    pub const STANDBY: &str = "standby";
    pub const MOTION_CAPABILITY_UNAVAILABLE: &str = "motion_capability_unavailable";
    pub const PHYSICAL_ESTOP_CAPABILITY_MISSING: &str = "physical_estop_capability_missing";
    pub const ESTOP_ACTIVE: &str = "estop_active";
    pub const STOP_CONFIRMATION_PENDING: &str = "stop_confirmation_pending";
    pub const FAULT_LATCHED: &str = "fault_latched";
    pub const SESSION_MISMATCH: &str = "session_mismatch";
    pub const MODE_MISMATCH: &str = "mode_mismatch";
    pub const MODE_EPOCH_MISMATCH: &str = "mode_epoch_mismatch";
    pub const MODE_TRANSITION_REQUIRES_STANDBY: &str = "mode_transition_requires_standby";
    pub const REQUEST_EXPIRED: &str = "request_expired";
    pub const REQUEST_FROM_FUTURE: &str = "request_from_future";
    pub const SOURCE_CLOCK_UNSUPPORTED: &str = "source_clock_unsupported";
    pub const RESOURCE_UNMAPPED: &str = "resource_unmapped";
    pub const NO_ACTIVE_MODE: &str = "no_active_mode";
    pub const REQUEST_CONFLICT: &str = "request_conflict";
    pub const MODE_TRANSITION_IN_PROGRESS: &str = "mode_transition_in_progress";
    pub const OPERATION_IN_PROGRESS: &str = "operation_in_progress";
    pub const STOP_CONFIRMATION_CAPABILITY_MISSING: &str = "stop_confirmation_capability_missing";
}

/// 车辆级 operation mode。
///
/// `Standby` 是唯一默认非运动稳态；active-to-active 直切由 supervisor 拒绝。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationMode {
    Standby = 0,
    Manual = 1,
    Auto = 2,
    Maintenance = 3,
    SelfTest = 4,
}

impl OperationMode {
    pub const fn is_active(self) -> bool {
        !matches!(self, Self::Standby)
    }
}

/// 覆盖 normal command 的安全状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SafetyState {
    Normal = 0,
    DegradedHold = 1,
    ControlledStop = 2,
    FaultLatched = 3,
    EstopLatched = 4,
}

/// supervisor 对 motion / operation step 的准入结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotionEnable {
    Allowed,
    Denied { reason_code: String },
}

/// 有完成语义的车辆级 operation 类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationKind {
    ModeTransition = 0,
    Stop = 1,
    DisableMotion = 2,
    ClearFaults = 3,
    Maintenance = 4,
    SelfTest = 5,
    /// Explicit authorization to claim resources for a normal control frame.
    /// It never follows implicitly from a mode transition or Stop grant.
    GenericClaim = 6,
}

/// operation 的异步进度阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationPhase {
    Accepted = 0,
    Stopping = 1,
    AwaitingStopConfirmation = 2,
    EnteringTargetMode = 3,
    Running = 4,
    SafeExit = 5,
    Completed = 6,
    Failed = 7,
    TimedOut = 8,
    Aborted = 9,
}

/// operation mode 切换的唯一外部请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeRequest {
    pub header: FrameHeader,
    pub valid_for_ms: u64,
    pub expected_mode: OperationMode,
    pub command_epoch: u64,
    pub target_mode: OperationMode,
    pub reason_code: String,
}

/// 请求清除已锁存且清除条件已消失的 fault。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClearFaultsRequest {
    pub header: FrameHeader,
    pub valid_for_ms: u64,
    pub expected_mode: OperationMode,
    pub command_epoch: u64,
    pub reason_code: String,
}

/// 只读地查询一个有完成语义 operation。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationQuery {
    pub request_id: u64,
    pub operation_id: String,
}

/// 只读查询当前 vehicle-level readiness。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadinessQuery {
    pub request_id: u64,
}

/// operation gate 的同步回执。
///
/// `Accepted` 只表示 operation 已进入 Server 的受控执行路径。它不表示 mode
/// 已切换、safe output 已提交或任意 backend 已确认停止。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationReceipt {
    pub receipt_id: String,
    pub request_id: u64,
    pub status: OperationReceiptStatus,
    pub operation_id: Option<String>,
    pub command_epoch: u64,
    pub reason_code: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationReceiptStatus {
    Accepted = 0,
    Rejected = 1,
}

/// operation 的异步、可审计状态事件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationEvent {
    pub event_id: String,
    pub operation_id: String,
    pub operation_kind: OperationKind,
    pub phase: OperationPhase,
    pub operation_mode: OperationMode,
    pub safety_state: SafetyState,
    pub command_epoch: u64,
    pub reason_code: String,
    pub server_timestamp_ns: u64,
    /// Present for Stop / Disable terminal or pending evidence; receipt acceptance alone
    /// never carries this result.
    pub stop_result: Option<StopOperationResult>,
}

/// 一项 backend 对受控停止或 safe exit 的观察结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendStopConfirmation {
    pub backend_id: String,
    pub status: BackendStopStatus,
    pub reason_code: String,
    pub observed_at_ns: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackendStopStatus {
    Pending = 0,
    Confirmed = 1,
    TimedOut = 2,
    Failed = 3,
    Unavailable = 4,
}

/// accepted Stop / Disable operation 的整车完成结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopOperationResult {
    pub operation_id: String,
    pub command_epoch: u64,
    pub status: StopOperationStatus,
    pub reason_code: String,
    pub backend_confirmations: Vec<BackendStopConfirmation>,
    pub server_timestamp_ns: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopOperationStatus {
    Pending = 0,
    Confirmed = 1,
    TimedOut = 2,
    Failed = 3,
}

/// control session 在 handshake 时冻结的权限交集。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionGrant {
    pub allowed_modes: Vec<OperationMode>,
    pub allowed_operations: Vec<OperationKind>,
    /// None means the runtime has no verified capability identity to publish.
    pub capability_hash: Option<String>,
}

/// 单个 backend 的 motion readiness 摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendReadinessSummary {
    pub backend_id: String,
    pub instance_id: String,
    pub instance_binding: String,
    pub resources: Vec<String>,
    pub ready_for_motion: bool,
    pub diagnostics_only: bool,
    pub feedback_freshness_ms: u64,
    pub safe_output: bool,
    pub stop_confirmation_available: bool,
    pub fault_reason_codes: Vec<String>,
    pub missing_capabilities: Vec<String>,
}

/// 只读 operation / safety / backend capability 视图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadinessSnapshot {
    pub schema_version: String,
    pub ready_for_motion: bool,
    pub motion_disabled_reason: Option<String>,
    pub release_id: Option<String>,
    pub profile_id: Option<String>,
    pub capability_hash: Option<String>,
    /// None means no verified checker registry identity is available to this endpoint.
    pub check_registry_hash: Option<String>,
    pub ignored_check_count: u64,
    pub reason_codes: Vec<String>,
    pub operation_mode: OperationMode,
    pub safety_state: SafetyState,
    pub motion_enable: MotionEnable,
    pub command_epoch: u64,
    pub active_operation_id: Option<String>,
    pub active_operation_phase: Option<OperationPhase>,
    pub backend_reports: Vec<BackendReadinessSummary>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generated::command::ClockDomain;
    use crate::transport::envelope::{MsgKind, WP_VER_CURRENT};

    #[test]
    fn lifecycle_wire_uses_v2_and_reserves_operation_request_kind() {
        assert_eq!(WP_VER_CURRENT, 0x02);
        assert!(MsgKind::from_u8(15).is_some());
    }

    fn header() -> FrameHeader {
        FrameHeader {
            request_id: 7,
            session_id: 11,
            source_timestamp_ns: 13,
            source_clock: ClockDomain::Monotonic,
        }
    }

    #[test]
    fn mode_request_roundtrip_keeps_compare_assertions() {
        let request = ModeRequest {
            header: header(),
            valid_for_ms: 250,
            expected_mode: OperationMode::Standby,
            command_epoch: 19,
            target_mode: OperationMode::Manual,
            reason_code: "operator_manual".to_owned(),
        };
        let decoded: ModeRequest =
            bincode::deserialize(&bincode::serialize(&request).unwrap()).unwrap();

        assert_eq!(decoded.header.request_id, request.header.request_id);
        assert_eq!(decoded.header.session_id, request.header.session_id);
        assert_eq!(decoded.valid_for_ms, request.valid_for_ms);
        assert_eq!(decoded.command_epoch, request.command_epoch);
        assert_eq!(decoded.target_mode, request.target_mode);
        assert_eq!(decoded.reason_code, request.reason_code);
        assert!(!decoded.expected_mode.is_active());
        assert!(decoded.target_mode.is_active());
    }

    #[test]
    fn accepted_stop_is_distinct_from_backend_confirmation() {
        let receipt = OperationReceipt {
            receipt_id: "receipt-1".to_owned(),
            request_id: 7,
            status: OperationReceiptStatus::Accepted,
            operation_id: Some("operation-1".to_owned()),
            command_epoch: 20,
            reason_code: reason::ACCEPTED.to_owned(),
        };
        let result = StopOperationResult {
            operation_id: "operation-1".to_owned(),
            command_epoch: 20,
            status: StopOperationStatus::Pending,
            reason_code: "awaiting_backend_confirmation".to_owned(),
            backend_confirmations: vec![BackendStopConfirmation {
                backend_id: "wheel".to_owned(),
                status: BackendStopStatus::Unavailable,
                reason_code: reason::STOP_CONFIRMATION_CAPABILITY_MISSING.to_owned(),
                observed_at_ns: 21,
            }],
            server_timestamp_ns: 21,
        };

        assert_eq!(receipt.status, OperationReceiptStatus::Accepted);
        assert_ne!(result.status, StopOperationStatus::Confirmed);
        assert_eq!(
            result.backend_confirmations[0].status,
            BackendStopStatus::Unavailable
        );
    }

    #[test]
    fn readiness_exposes_missing_stop_confirmation_as_motion_denial() {
        let readiness = ReadinessSnapshot {
            schema_version: "ranger.telemetry_diagnostics.v2".to_owned(),
            ready_for_motion: false,
            motion_disabled_reason: Some(reason::STOP_CONFIRMATION_CAPABILITY_MISSING.to_owned()),
            release_id: None,
            profile_id: None,
            capability_hash: None,
            check_registry_hash: Some("sha256:checks".to_owned()),
            ignored_check_count: 0,
            reason_codes: vec![reason::STOP_CONFIRMATION_CAPABILITY_MISSING.to_owned()],
            operation_mode: OperationMode::Standby,
            safety_state: SafetyState::Normal,
            motion_enable: MotionEnable::Denied {
                reason_code: reason::STOP_CONFIRMATION_CAPABILITY_MISSING.to_owned(),
            },
            command_epoch: 1,
            active_operation_id: None,
            active_operation_phase: None,
            backend_reports: vec![BackendReadinessSummary {
                backend_id: "wheel".to_owned(),
                instance_id: "wheel0".to_owned(),
                instance_binding: "can0".to_owned(),
                resources: vec!["vehicle.motion".to_owned()],
                ready_for_motion: false,
                diagnostics_only: false,
                feedback_freshness_ms: 0,
                safe_output: false,
                stop_confirmation_available: false,
                fault_reason_codes: vec![],
                missing_capabilities: vec![reason::STOP_CONFIRMATION_CAPABILITY_MISSING.to_owned()],
            }],
        };

        assert!(matches!(
            readiness.motion_enable,
            MotionEnable::Denied { .. }
        ));
        assert!(!readiness.backend_reports[0].stop_confirmation_available);
    }
}
