use std::os::fd::{AsRawFd, OwnedFd};
use std::thread;
use std::time::Instant;

use contracts::generated::command::{
    ClockDomain, ControlFrame, EhaFaultResetCommand, Resource, StopCommand,
};
use contracts::generated::receipt::{GateReceipt, ReceiptStatus};
use contracts::generated::session::{
    Heartbeat, HeartbeatAck, SessionHandshake, SessionResponse, SessionStatus,
};
use contracts::transport::bootstrap::{
    ConnectRequest, ConnectResponse, ConnectStatusRepr, SessionTypeRepr,
};
use contracts::transport::envelope::{Envelope, HEADER_LEN, MsgKind};
use nix::sys::socket::{AddressFamily, MsgFlags, SockFlag, SockType, recv, send, socketpair};

use super::{ControlClient, VehicleMotionCommand};

const MAX_FRAME_BYTES: u32 = 65_536;

#[test]
fn control_client_builds_motion_frames_and_sequences_requests() {
    let (client_fd, server_fd) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let server = thread::spawn(move || mock_server(server_fd));

    let mut client = ControlClient::from_connected_fd(client_fd, MAX_FRAME_BYTES).unwrap();
    client.last_heartbeat_at = Instant::now()
        .checked_sub(client.heartbeat_interval)
        .unwrap();

    let first = client
        .send_motion(
            "yard-low-speed.v1",
            VehicleMotionCommand {
                linear_velocity_mps: 0.8,
                yaw_rate_radps: 0.15,
            },
        )
        .unwrap();
    assert_eq!(first.request_id, 1);

    let second = client
        .send_motion(
            "yard-low-speed.v1",
            VehicleMotionCommand {
                linear_velocity_mps: 0.4,
                yaw_rate_radps: -0.1,
            },
        )
        .unwrap();
    assert_eq!(second.request_id, 2);

    client.last_heartbeat_at = Instant::now()
        .checked_sub(client.heartbeat_interval)
        .unwrap();
    let stop = client.stop("test_stop").unwrap();
    assert_eq!(stop.request_id, 3);
    server.join().unwrap();
}

#[test]
fn reset_eha_fault_roundtrip_and_local_reason_validation() {
    let (client_fd, server_fd) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let server = thread::spawn(move || reset_mock_server(server_fd));

    let mut client = ControlClient::from_connected_fd(client_fd, MAX_FRAME_BYTES).unwrap();

    // 空 reason 在发送前被本地拒绝。
    let error = client.reset_eha_fault("").unwrap_err();
    assert!(matches!(
        error,
        super::ClientError::EmptyEhaFaultResetReason
    ));

    let receipt = client.reset_eha_fault("fault_handled").unwrap();
    assert_eq!(receipt.request_id, 1);
    assert_eq!(receipt.status, ReceiptStatus::Accepted);
    server.join().unwrap();
}

fn reset_mock_server(stream: OwnedFd) {
    handshake(&stream);
    let envelope = receive(&stream);
    assert_eq!(envelope.msg_kind, MsgKind::EhaFaultResetCommand);
    let reset: EhaFaultResetCommand = decode(&envelope);
    assert_eq!(reset.header.request_id, 1);
    assert_eq!(reset.header.session_id, 42);
    assert_eq!(reset.header.source_clock, ClockDomain::Monotonic);
    assert!(reset.header.source_timestamp_ns > 0);
    assert_eq!(reset.reason_code, "fault_handled");
    send_receipt(&stream, 1);
}

fn handshake(stream: &OwnedFd) {
    let envelope = receive(stream);
    assert_eq!(envelope.msg_kind, MsgKind::ConnectRequest);
    let request: ConnectRequest = decode(&envelope);
    assert!(request.schemas.is_empty());
    respond(
        stream,
        MsgKind::ConnectResponse,
        &ConnectResponse {
            server_version: "test".to_owned(),
            schemas: Vec::new(),
            allowed_session_types: vec![SessionTypeRepr::Control],
            status: ConnectStatusRepr::Ok,
        },
    );

    let envelope = receive(stream);
    assert_eq!(envelope.msg_kind, MsgKind::SessionHandshake);
    let handshake: SessionHandshake = decode(&envelope);
    assert_eq!(handshake.session_type, SessionTypeRepr::Control);
    respond(
        stream,
        MsgKind::SessionResponse,
        &SessionResponse {
            session_id: 42,
            status: SessionStatus::Ok,
            heartbeat_interval_ms: 200,
            heartbeat_timeout_ms: 1_000,
            command_default_valid_for_ms: 200,
            command_max_valid_for_ms: 500,
        },
    );
}

fn mock_server(stream: OwnedFd) {
    handshake(&stream);

    assert_heartbeat(&stream);

    assert_motion_frame(&stream, 1, 1, 0.8, 0.15);
    assert_motion_frame(&stream, 2, 2, 0.4, -0.1);
    assert_heartbeat(&stream);

    let envelope = receive(&stream);
    assert_eq!(envelope.msg_kind, MsgKind::StopCommand);
    let stop: StopCommand = decode(&envelope);
    assert_eq!(stop.header.request_id, 3);
    assert_eq!(stop.header.session_id, 42);
    assert_eq!(stop.reason_code, "test_stop");
    send_receipt(&stream, 3);
}

fn assert_heartbeat(stream: &OwnedFd) {
    let envelope = receive(stream);
    assert_eq!(envelope.msg_kind, MsgKind::Heartbeat);
    let heartbeat: Heartbeat = decode(&envelope);
    assert!(heartbeat.timestamp_ns > 0);
    respond(
        stream,
        MsgKind::HeartbeatAck,
        &HeartbeatAck {
            server_timestamp_ns: heartbeat.timestamp_ns,
        },
    );
}

fn assert_motion_frame(
    stream: &OwnedFd,
    request_id: u64,
    sequence: u64,
    linear_velocity_mps: f64,
    yaw_rate_radps: f64,
) {
    let envelope = receive(stream);
    assert_eq!(envelope.msg_kind, MsgKind::ControlFrame);
    let frame: ControlFrame = decode(&envelope);
    assert_eq!(frame.header.request_id, request_id);
    assert_eq!(frame.header.session_id, 42);
    assert_eq!(frame.header.source_clock, ClockDomain::Monotonic);
    assert!(frame.header.source_timestamp_ns > 0);
    assert_eq!(frame.sequence, sequence);
    assert_eq!(frame.valid_for_ms, 200);
    assert_eq!(frame.profile_id, "yard-low-speed.v1");
    assert_eq!(frame.commands.len(), 1);

    let command = &frame.commands[0];
    assert_eq!(command.resource, Resource::VehicleMotion);
    assert_eq!(command.target.fields.len(), 2);
    assert_eq!(command.target.fields[0].0, "linear_velocity_mps");
    assert_eq!(command.target.fields[0].1, linear_velocity_mps);
    assert_eq!(command.target.fields[1].0, "yaw_rate_radps");
    assert_eq!(command.target.fields[1].1, yaw_rate_radps);
    assert_eq!(command.units[0].field, "linear_velocity_mps");
    assert_eq!(command.units[0].unit, "m/s");
    assert_eq!(command.units[1].field, "yaw_rate_radps");
    assert_eq!(command.units[1].unit, "rad/s");
    send_receipt(stream, request_id);
}

fn send_receipt(stream: &OwnedFd, request_id: u64) {
    respond(
        stream,
        MsgKind::GateReceipt,
        &GateReceipt {
            receipt_id: format!("receipt-{request_id}"),
            request_id,
            status: ReceiptStatus::Accepted,
            reason_code: "ok".to_owned(),
            affected_resource: None,
        },
    );
}

fn receive(stream: &OwnedFd) -> Envelope {
    let mut buffer = vec![0_u8; HEADER_LEN + MAX_FRAME_BYTES as usize];
    let received = recv(stream.as_raw_fd(), &mut buffer, MsgFlags::empty()).unwrap();
    Envelope::decode(&buffer[..received], MAX_FRAME_BYTES).unwrap()
}

fn decode<T: serde::de::DeserializeOwned>(envelope: &Envelope) -> T {
    contracts::transport::codec::deserialize(&envelope.payload, u64::from(MAX_FRAME_BYTES)).unwrap()
}

fn respond<T: serde::Serialize>(stream: &OwnedFd, kind: MsgKind, payload: &T) {
    let bytes = Envelope::with_bincode_payload(kind, payload)
        .unwrap()
        .encode();
    assert_eq!(
        send(stream.as_raw_fd(), &bytes, MsgFlags::empty()).unwrap(),
        bytes.len()
    );
}
