//! v2 Session handshake、frozen grant 和 heartbeat DTO。

use serde::{Deserialize, Serialize};

use super::super::transport::bootstrap::SessionTypeRepr;
use super::lifecycle::{OperationMode, SessionGrant};

/// Session 建立请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHandshake {
    pub session_type: SessionTypeRepr,
}

/// Session 建立响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionResponse {
    pub session_id: u64,
    pub status: SessionStatus,
    pub heartbeat_interval_ms: u64,
    pub heartbeat_timeout_ms: u64,
    pub command_default_valid_for_ms: u64,
    pub command_max_valid_for_ms: u64,
    /// Frozen authorization intersection for this session.
    pub grant: SessionGrant,
    /// The current supervisor state is explicit; session creation never enters an active mode.
    pub operation_mode: OperationMode,
    pub command_epoch: u64,
}

/// Session 响应状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    Ok = 0,
    SessionAlreadyActive = 1,
    VehicleNotReady = 2,
    SessionTypeNotAllowed = 3,
    SchemaIncompatible = 4,
    SessionLimitReached = 5,
}

/// Heartbeat payload（空，只需 msg_kind）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heartbeat {
    pub timestamp_ns: u64,
}

/// HeartbeatAck payload。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatAck {
    pub server_timestamp_ns: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_handshake_roundtrip() {
        let hs = SessionHandshake {
            session_type: SessionTypeRepr::Control,
        };
        let bytes = bincode::serialize(&hs).unwrap();
        let decoded: SessionHandshake = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded.session_type, SessionTypeRepr::Control);
    }

    #[test]
    fn session_response_roundtrip() {
        let resp = SessionResponse {
            session_id: 12345,
            status: SessionStatus::Ok,
            heartbeat_interval_ms: 200,
            heartbeat_timeout_ms: 1000,
            command_default_valid_for_ms: 200,
            command_max_valid_for_ms: 500,
            grant: SessionGrant {
                allowed_modes: vec![OperationMode::Manual],
                allowed_operations: vec![],
                capability_hash: Some("sha256:capability".to_owned()),
            },
            operation_mode: OperationMode::Standby,
            command_epoch: 1,
        };
        let bytes = bincode::serialize(&resp).unwrap();
        let decoded: SessionResponse = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded.session_id, 12345);
        assert_eq!(decoded.heartbeat_interval_ms, 200);
        assert_eq!(decoded.operation_mode, OperationMode::Standby);
    }
}
