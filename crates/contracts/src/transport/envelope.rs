//! Bootstrap envelope：固定 11 字节 header + payload。
//!
//! 所有消息（包括 schema 协商后的）使用同一个 envelope。
//! wire protocol version (wp_ver) 的 breaking change 递增，旧 client 连不上。

use std::fmt;

/// 固定 magic bytes。
pub const MAGIC: [u8; 4] = *b"RNG1";

/// 当前 wire protocol version。
pub const WP_VER_CURRENT: u8 = 0x01;

/// Envelope header 固定 11 字节。
pub const HEADER_LEN: usize = 4 + 1 + 1 + 4; // magic + wp_ver + msg_kind + payload_len

/// 消息类型枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MsgKind {
    ConnectRequest = 1,
    ConnectResponse = 2,
    SessionHandshake = 3,
    SessionResponse = 4,
    Heartbeat = 5,
    HeartbeatAck = 6,
    ControlFrame = 7,
    StopCommand = 8,
    GateReceipt = 9,
    ExecutionEvent = 10,
    DiagnosticsQuery = 11,
    DiagnosticsSnapshot = 12,
    ServerError = 13,
    EhaFaultResetCommand = 14,
}

impl MsgKind {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::ConnectRequest),
            2 => Some(Self::ConnectResponse),
            3 => Some(Self::SessionHandshake),
            4 => Some(Self::SessionResponse),
            5 => Some(Self::Heartbeat),
            6 => Some(Self::HeartbeatAck),
            7 => Some(Self::ControlFrame),
            8 => Some(Self::StopCommand),
            9 => Some(Self::GateReceipt),
            10 => Some(Self::ExecutionEvent),
            11 => Some(Self::DiagnosticsQuery),
            12 => Some(Self::DiagnosticsSnapshot),
            13 => Some(Self::ServerError),
            14 => Some(Self::EhaFaultResetCommand),
            _ => None,
        }
    }

    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

/// Envelope 解码错误。
#[derive(Debug, thiserror::Error)]
pub enum EnvelopeError {
    #[error("magic 不匹配：期望 {expected:?}，实际 {actual:?}")]
    MagicMismatch { expected: [u8; 4], actual: [u8; 4] },

    #[error("wire protocol version 不支持：{0}")]
    UnsupportedWpVer(u8),

    #[error("未知消息类型：{0}")]
    UnknownMsgKind(u8),

    #[error("payload 长度与 packet 实际长度不一致：声明 {declared}，实际 {actual}")]
    LengthMismatch { declared: u32, actual: usize },

    #[error("payload 超过上限：{actual} > {max}")]
    FrameTooLarge { actual: u32, max: u32 },

    #[error("SOCK_SEQPACKET packet 被截断")]
    FrameTruncated,

    #[error("header 解码失败：packet 短于 {HEADER_LEN} 字节")]
    HeaderTooShort,

    #[error("payload Bincode 解码失败：{0}")]
    DecodeFailed(String),
}

/// 解码后的 envelope。
#[derive(Debug, Clone)]
pub struct Envelope {
    pub wp_ver: u8,
    pub msg_kind: MsgKind,
    pub payload: Vec<u8>,
}

impl Envelope {
    /// 从 SOCK_SEQPACKET packet（含 header + payload）解码。
    ///
    /// `max_frame_bytes` 为 payload 上限。
    pub fn decode(packet: &[u8], max_frame_bytes: u32) -> Result<Self, EnvelopeError> {
        if packet.len() < HEADER_LEN {
            return Err(EnvelopeError::HeaderTooShort);
        }

        let magic: [u8; 4] = packet[0..4].try_into().unwrap();
        if magic != MAGIC {
            return Err(EnvelopeError::MagicMismatch {
                expected: MAGIC,
                actual: magic,
            });
        }

        let wp_ver = packet[4];
        if wp_ver != WP_VER_CURRENT {
            return Err(EnvelopeError::UnsupportedWpVer(wp_ver));
        }

        let msg_kind_raw = packet[5];
        let msg_kind =
            MsgKind::from_u8(msg_kind_raw).ok_or(EnvelopeError::UnknownMsgKind(msg_kind_raw))?;

        let payload_len = u32::from_be_bytes(packet[6..10].try_into().unwrap());

        if payload_len > max_frame_bytes {
            return Err(EnvelopeError::FrameTooLarge {
                actual: payload_len,
                max: max_frame_bytes,
            });
        }

        let payload_start = HEADER_LEN;
        let payload_end = payload_start + payload_len as usize;
        if payload_end != packet.len() {
            return Err(EnvelopeError::LengthMismatch {
                declared: payload_len,
                actual: packet.len().saturating_sub(payload_start),
            });
        }

        let payload = packet[payload_start..payload_end].to_vec();
        Ok(Self {
            wp_ver,
            msg_kind,
            payload,
        })
    }

    /// 编码为 wire bytes（header + payload）。
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(HEADER_LEN + self.payload.len());
        buf.extend_from_slice(&MAGIC);
        buf.push(self.wp_ver);
        buf.push(self.msg_kind.to_u8());
        buf.extend_from_slice(&(self.payload.len() as u32).to_be_bytes());
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// 用 Bincode options 编码 payload 并封装为 envelope。
    pub fn with_bincode_payload<T: serde::Serialize>(
        msg_kind: MsgKind,
        payload: &T,
    ) -> Result<Self, EnvelopeError> {
        let bytes = super::codec::serialize(payload)
            .map_err(|e| EnvelopeError::DecodeFailed(e.to_string()))?;
        Ok(Self {
            wp_ver: WP_VER_CURRENT,
            msg_kind,
            payload: bytes,
        })
    }
}

impl fmt::Display for MsgKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ConnectRequest => "ConnectRequest",
            Self::ConnectResponse => "ConnectResponse",
            Self::SessionHandshake => "SessionHandshake",
            Self::SessionResponse => "SessionResponse",
            Self::Heartbeat => "Heartbeat",
            Self::HeartbeatAck => "HeartbeatAck",
            Self::ControlFrame => "ControlFrame",
            Self::StopCommand => "StopCommand",
            Self::GateReceipt => "GateReceipt",
            Self::ExecutionEvent => "ExecutionEvent",
            Self::DiagnosticsQuery => "DiagnosticsQuery",
            Self::DiagnosticsSnapshot => "DiagnosticsSnapshot",
            Self::ServerError => "ServerError",
            Self::EhaFaultResetCommand => "EhaFaultResetCommand",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_minimal_envelope() {
        let env = Envelope {
            wp_ver: WP_VER_CURRENT,
            msg_kind: MsgKind::Heartbeat,
            payload: vec![],
        };
        let bytes = env.encode();
        assert_eq!(bytes.len(), HEADER_LEN);

        let decoded = Envelope::decode(&bytes, 65536).unwrap();
        assert_eq!(decoded.msg_kind, MsgKind::Heartbeat);
        assert!(decoded.payload.is_empty());
    }

    #[test]
    fn reject_bad_magic() {
        let bytes = vec![0xDE, 0xAD, 0xBE, 0xEF, WP_VER_CURRENT, 1, 0, 0, 0, 0];
        let err = Envelope::decode(&bytes, 65536).unwrap_err();
        assert!(matches!(err, EnvelopeError::MagicMismatch { .. }));
    }

    #[test]
    fn reject_unsupported_wp_ver() {
        let mut bytes = vec![];
        bytes.extend_from_slice(&MAGIC);
        bytes.push(0x99);
        bytes.push(1);
        bytes.extend_from_slice(&0u32.to_be_bytes());
        let err = Envelope::decode(&bytes, 65536).unwrap_err();
        assert!(matches!(err, EnvelopeError::UnsupportedWpVer(0x99)));
    }

    #[test]
    fn reject_unknown_msg_kind() {
        let mut bytes = vec![];
        bytes.extend_from_slice(&MAGIC);
        bytes.push(WP_VER_CURRENT);
        bytes.push(0xFF);
        bytes.extend_from_slice(&0u32.to_be_bytes());
        let err = Envelope::decode(&bytes, 65536).unwrap_err();
        assert!(matches!(err, EnvelopeError::UnknownMsgKind(0xFF)));
    }

    #[test]
    fn reject_payload_length_mismatch() {
        let mut bytes = vec![];
        bytes.extend_from_slice(&MAGIC);
        bytes.push(WP_VER_CURRENT);
        bytes.push(1);
        bytes.extend_from_slice(&100u32.to_be_bytes()); // 声明 100 字节
        // 但没有 payload
        let err = Envelope::decode(&bytes, 65536).unwrap_err();
        assert!(matches!(
            err,
            EnvelopeError::LengthMismatch {
                declared: 100,
                actual: 0
            }
        ));
    }

    #[test]
    fn reject_frame_too_large() {
        let mut bytes = vec![];
        bytes.extend_from_slice(&MAGIC);
        bytes.push(WP_VER_CURRENT);
        bytes.push(1);
        bytes.extend_from_slice(&1000u32.to_be_bytes());
        bytes.extend_from_slice(&[0u8; 1000]);
        let err = Envelope::decode(&bytes, 512).unwrap_err();
        assert!(matches!(
            err,
            EnvelopeError::FrameTooLarge {
                actual: 1000,
                max: 512
            }
        ));
    }

    #[test]
    fn reject_header_too_short() {
        let bytes = [0u8; 5];
        let err = Envelope::decode(&bytes, 65536).unwrap_err();
        assert!(matches!(err, EnvelopeError::HeaderTooShort));
    }

    #[test]
    fn roundtrip_with_payload() {
        let env = Envelope {
            wp_ver: WP_VER_CURRENT,
            msg_kind: MsgKind::ControlFrame,
            payload: vec![1, 2, 3, 4, 5],
        };
        let bytes = env.encode();
        let decoded = Envelope::decode(&bytes, 65536).unwrap();
        assert_eq!(decoded.payload, vec![1, 2, 3, 4, 5]);
    }
}
