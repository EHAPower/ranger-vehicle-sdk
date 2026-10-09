//! v1 GateReceipt 和 Stop 专用 ExecutionEvent DTO。
//!
//! 对应 `sdk/schema/receipt_event.yaml`。

use serde::{Deserialize, Serialize};

/// Gate 同步 receipt。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateReceipt {
    pub receipt_id: String,
    pub request_id: u64,
    pub status: ReceiptStatus,
    pub reason_code: String,
    pub affected_resource: Option<String>,
}

/// receipt 状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReceiptStatus {
    Accepted = 0,
    Rejected = 1,
}

/// v1 Stop 专用异步 execution event。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopExecutionEvent {
    pub event_id: String,
    pub request_id: u64,
    pub command_epoch: u64,
    pub status: StopEventStatus,
    pub reason: String,
    pub confirmation_source: Option<String>,
    pub server_timestamp_ns: u64,
}

/// Stop event 状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopEventStatus {
    /// Backend 确认已停止。
    Observed = 0,
    /// Backend 确认超时。
    TimedOut = 1,
    /// Backend 提交失败。
    Failed = 2,
}

/// v1 Gate reject 的稳定 reason code。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    // session
    NoActiveControlSession,
    SessionExpired,
    SessionTypeNotAllowed,
    // schema
    SchemaIncompatible,
    // sequence
    SequenceTooOld,
    SequenceConflict,
    // frame
    EmptyControlFrame,
    // validity
    ValidForMsMissing,
    ValidForMsZero,
    ValidForMsExceedsMax,
    // freshness
    StaleOnArrival,
    SourceTimestampInFuture,
    SourceClockUnspecified,
    // mode/safety
    ModeNotAllowed,
    SafetyStateBlocksMotion,
    EStopActive,
    FaultLatched,
    StopPending,
    // resource
    ResourceConflict,
    ResourceNotClaimed,
    // capability/readiness
    CapabilityNotReady,
    ProfileMismatch,
    CalibrationMismatch,
    // limits
    HardLimitExceeded,
    // unit
    UnitMissing,
    UnitMismatch,
    // rt mapping
    RtSnapshotMappingFailed,
    CommandCountExceeded,
}

impl RejectReason {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoActiveControlSession => "no_active_control_session",
            Self::SessionExpired => "session_expired",
            Self::SessionTypeNotAllowed => "session_type_not_allowed",
            Self::SchemaIncompatible => "schema_incompatible",
            Self::SequenceTooOld => "sequence_too_old",
            Self::SequenceConflict => "sequence_conflict",
            Self::EmptyControlFrame => "empty_control_frame",
            Self::ValidForMsMissing => "valid_for_ms_missing",
            Self::ValidForMsZero => "valid_for_ms_zero",
            Self::ValidForMsExceedsMax => "valid_for_ms_exceeds_max",
            Self::StaleOnArrival => "stale_on_arrival",
            Self::SourceTimestampInFuture => "source_timestamp_in_future",
            Self::SourceClockUnspecified => "source_clock_unspecified",
            Self::ModeNotAllowed => "mode_not_allowed",
            Self::SafetyStateBlocksMotion => "safety_state_blocks_motion",
            Self::EStopActive => "estop_active",
            Self::FaultLatched => "fault_latched",
            Self::StopPending => "stop_pending",
            Self::ResourceConflict => "resource_conflict",
            Self::ResourceNotClaimed => "resource_not_claimed",
            Self::CapabilityNotReady => "capability_not_ready",
            Self::ProfileMismatch => "profile_mismatch",
            Self::CalibrationMismatch => "calibration_mismatch",
            Self::HardLimitExceeded => "hard_limit_exceeded",
            Self::UnitMissing => "unit_missing",
            Self::UnitMismatch => "unit_mismatch",
            Self::RtSnapshotMappingFailed => "rt_snapshot_mapping_failed",
            Self::CommandCountExceeded => "command_count_exceeded",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_receipt_accepted_roundtrip() {
        let receipt = GateReceipt {
            receipt_id: "rcpt-1".to_string(),
            request_id: 42,
            status: ReceiptStatus::Accepted,
            reason_code: "ok".to_string(),
            affected_resource: None,
        };
        let bytes = bincode::serialize(&receipt).unwrap();
        let decoded: GateReceipt = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded.status, ReceiptStatus::Accepted);
    }

    #[test]
    fn gate_receipt_rejected_roundtrip() {
        let receipt = GateReceipt {
            receipt_id: "rcpt-2".to_string(),
            request_id: 43,
            status: ReceiptStatus::Rejected,
            reason_code: RejectReason::EmptyControlFrame.code().to_string(),
            affected_resource: Some("vehicle.motion".to_string()),
        };
        let bytes = bincode::serialize(&receipt).unwrap();
        let decoded: GateReceipt = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded.reason_code, "empty_control_frame");
    }

    #[test]
    fn stop_execution_event_roundtrip() {
        let event = StopExecutionEvent {
            event_id: "evt-1".to_string(),
            request_id: 100,
            command_epoch: 5,
            status: StopEventStatus::Observed,
            reason: "stop_confirmed".to_string(),
            confirmation_source: Some("backend.eha".to_string()),
            server_timestamp_ns: 3_000_000_000,
        };
        let bytes = bincode::serialize(&event).unwrap();
        let decoded: StopExecutionEvent = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded.status, StopEventStatus::Observed);
    }
}
