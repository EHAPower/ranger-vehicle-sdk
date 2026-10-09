use std::os::fd::{AsRawFd, OwnedFd};
use std::thread;
use std::time::Instant;

use contracts::generated::command::{
    ClockDomain, ControlFrame, EhaFaultResetCommand, Resource, StopCommand,
};
use contracts::generated::lifecycle::{
    BackendReadinessSummary, MotionEnable, OperationEvent, OperationKind, OperationMode,
    OperationPhase, OperationReceipt, OperationReceiptStatus, ReadinessSnapshot, SafetyState,
    SessionGrant,
};
use contracts::generated::receipt::{GateReceipt, ReceiptStatus};
use contracts::generated::session::{
    Heartbeat, HeartbeatAck, SessionHandshake, SessionResponse, SessionStatus,
};
use contracts::transport::bootstrap::{
    ConnectRequest, ConnectResponse, ConnectStatusRepr, SessionTypeRepr,
};
use contracts::transport::envelope::{Envelope, HEADER_LEN, MsgKind};
use contracts::transport::schema_manifest::{
    negotiate_strict_api_schemas, strict_api_schema_manifest,
};
use nix::sys::socket::{AddressFamily, MsgFlags, SockFlag, SockType, recv, send, socketpair};

use super::{ControlClient, VehicleMotionCommand};

const MAX_FRAME_BYTES: u32 = 65_536;

#[test]
fn connection_rejects_empty_server_schema_negotiation() {
    let (client_fd, server_fd) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let server = thread::spawn(move || {
        let envelope = receive(&server_fd);
        assert_eq!(envelope.msg_kind, MsgKind::ConnectRequest);
        respond(
            &server_fd,
            MsgKind::ConnectResponse,
            &ConnectResponse {
                server_version: "test".to_owned(),
                schemas: Vec::new(),
                allowed_session_types: vec![SessionTypeRepr::Control],
                status: ConnectStatusRepr::Ok,
            },
        );
    });

    let result = ControlClient::from_connected_fd(client_fd, MAX_FRAME_BYTES);
    assert!(matches!(
        result,
        Err(super::ClientError::Protocol(message)) if message.contains("schema negotiation")
    ));
    server.join().unwrap();
}

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
            OperationMode::Manual,
            9,
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
            OperationMode::Manual,
            9,
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
    let stop = client.stop(OperationMode::Manual, 9, "test_stop").unwrap();
    assert_eq!(stop.request_id, 3);
    assert_eq!(stop.status, OperationReceiptStatus::Accepted);
    let operation_id = stop.operation_id.as_deref().unwrap();
    client.last_heartbeat_at = Instant::now()
        .checked_sub(client.heartbeat_interval)
        .unwrap();
    let event = client.query_operation(operation_id).unwrap();
    assert_eq!(event.phase, OperationPhase::Completed);
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

#[test]
fn lifecycle_requests_keep_mode_epoch_explicit_and_readiness_reports_denial() {
    let (client_fd, server_fd) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let server = thread::spawn(move || lifecycle_mock_server(server_fd));
    let mut client = ControlClient::from_connected_fd(client_fd, MAX_FRAME_BYTES).unwrap();

    let mode = client
        .request_mode(
            OperationMode::Standby,
            9,
            OperationMode::Manual,
            "operator_manual",
        )
        .unwrap();
    assert_eq!(mode.status, OperationReceiptStatus::Accepted);

    let clear = client
        .clear_faults(OperationMode::Standby, 10, "operator_clear")
        .unwrap();
    assert_eq!(clear.status, OperationReceiptStatus::Rejected);

    client.last_heartbeat_at = Instant::now()
        .checked_sub(client.heartbeat_interval)
        .unwrap();
    let readiness = client.readiness().unwrap();
    assert!(!readiness.ready_for_motion);
    assert!(matches!(
        readiness.motion_enable,
        MotionEnable::Denied { .. }
    ));
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
    let expected = strict_api_schema_manifest();
    assert_eq!(request.schemas.len(), expected.len());
    let (status, schemas) = negotiate_strict_api_schemas(&request.schemas);
    assert_eq!(status, ConnectStatusRepr::Ok);
    respond(
        stream,
        MsgKind::ConnectResponse,
        &ConnectResponse {
            server_version: "test".to_owned(),
            schemas,
            allowed_session_types: vec![SessionTypeRepr::Control],
            status,
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
            grant: SessionGrant {
                allowed_modes: vec![OperationMode::Manual],
                allowed_operations: vec![OperationKind::Stop],
                capability_hash: Some("sha256:capability".to_owned()),
            },
            operation_mode: OperationMode::Standby,
            command_epoch: 9,
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
    assert_eq!(stop.sequence, 3);
    assert_eq!(stop.expected_mode, OperationMode::Manual);
    assert_eq!(stop.command_epoch, 9);
    respond(
        &stream,
        MsgKind::OperationReceipt,
        &OperationReceipt {
            receipt_id: "operation-receipt-3".to_owned(),
            request_id: 3,
            status: OperationReceiptStatus::Accepted,
            operation_id: Some("operation-3".to_owned()),
            command_epoch: 10,
            reason_code: "accepted".to_owned(),
        },
    );

    assert_heartbeat(&stream);
    let query = receive(&stream);
    assert_eq!(query.msg_kind, MsgKind::OperationQuery);
    let query: contracts::generated::lifecycle::OperationQuery = decode(&query);
    assert_eq!(query.operation_id, "operation-3");
    respond(
        &stream,
        MsgKind::OperationEvent,
        &OperationEvent {
            event_id: "event-3".to_owned(),
            operation_id: "operation-3".to_owned(),
            operation_kind: OperationKind::Stop,
            phase: OperationPhase::Completed,
            operation_mode: OperationMode::Standby,
            safety_state: SafetyState::Normal,
            command_epoch: 10,
            reason_code: "stop_confirmed".to_owned(),
            server_timestamp_ns: 1,
            stop_result: None,
        },
    );
}

fn lifecycle_mock_server(stream: OwnedFd) {
    handshake(&stream);

    let envelope = receive(&stream);
    assert_eq!(envelope.msg_kind, MsgKind::ModeRequest);
    let request: contracts::generated::lifecycle::ModeRequest = decode(&envelope);
    assert_eq!(request.expected_mode, OperationMode::Standby);
    assert_eq!(request.command_epoch, 9);
    assert_eq!(request.target_mode, OperationMode::Manual);
    respond(
        &stream,
        MsgKind::OperationReceipt,
        &OperationReceipt {
            receipt_id: "mode-receipt".to_owned(),
            request_id: request.header.request_id,
            status: OperationReceiptStatus::Accepted,
            operation_id: Some("mode-operation".to_owned()),
            command_epoch: 10,
            reason_code: "accepted".to_owned(),
        },
    );

    let envelope = receive(&stream);
    assert_eq!(envelope.msg_kind, MsgKind::ClearFaultsRequest);
    let clear: contracts::generated::lifecycle::ClearFaultsRequest = decode(&envelope);
    assert_eq!(clear.expected_mode, OperationMode::Standby);
    assert_eq!(clear.command_epoch, 10);
    respond(
        &stream,
        MsgKind::OperationReceipt,
        &OperationReceipt {
            receipt_id: "clear-receipt".to_owned(),
            request_id: clear.header.request_id,
            status: OperationReceiptStatus::Rejected,
            operation_id: None,
            command_epoch: 10,
            reason_code: "fault_blocker_active".to_owned(),
        },
    );

    assert_heartbeat(&stream);
    let envelope = receive(&stream);
    assert_eq!(envelope.msg_kind, MsgKind::ReadinessQuery);
    let query: contracts::generated::lifecycle::ReadinessQuery = decode(&envelope);
    assert_eq!(query.request_id, 3);
    respond(
        &stream,
        MsgKind::ReadinessSnapshot,
        &ReadinessSnapshot {
            schema_version: "ranger.telemetry_diagnostics.v2".to_owned(),
            ready_for_motion: false,
            motion_disabled_reason: Some("motion_capability_unavailable".to_owned()),
            release_id: None,
            profile_id: None,
            capability_hash: None,
            check_registry_hash: Some("sha256:checks".to_owned()),
            ignored_check_count: 0,
            reason_codes: vec!["motion_capability_unavailable".to_owned()],
            operation_mode: OperationMode::Standby,
            safety_state: SafetyState::DegradedHold,
            motion_enable: MotionEnable::Denied {
                reason_code: "motion_capability_unavailable".to_owned(),
            },
            command_epoch: 10,
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
                missing_capabilities: vec!["stop_confirmation_capability_missing".to_owned()],
            }],
        },
    );
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
    assert_eq!(frame.expected_mode, OperationMode::Manual);
    assert_eq!(frame.command_epoch, 9);
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
