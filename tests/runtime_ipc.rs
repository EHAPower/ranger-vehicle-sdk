// Copyright The ranger Contributors

use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use ranger_vehicle_sdk::{
    MAX_FRAME, Operation, Outcome, PROTOCOL, Request, Response, RuntimeStatus, call, read_frame,
    write_frame,
};

fn status() -> RuntimeStatus {
    RuntimeStatus {
        instance_id: "mock-runtime".into(),
        backend: "mock".into(),
        worker_count: 1,
        probe_count: 0,
        ready: true,
        motion_capable: false,
    }
}

fn response(request_id: u64) -> Response {
    Response {
        protocol: PROTOCOL.into(),
        request_id,
        outcome: Outcome::Ok,
        runtime: Some(status()),
    }
}

fn socket_path() -> PathBuf {
    static NEXT_SOCKET: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "ranger-vehicle-sdk-{}-{}.sock",
        std::process::id(),
        NEXT_SOCKET.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn write_frame_rejects_values_larger_than_max_frame() {
    let oversized = "x".repeat(MAX_FRAME);
    let error = write_frame(&mut Vec::new(), &oversized).expect_err("oversized JSON must fail");
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
}

#[test]
fn read_frame_rejects_length_larger_than_max_frame() {
    let bytes = ((MAX_FRAME + 1) as u32).to_be_bytes().to_vec();
    let error =
        read_frame::<Request>(&mut bytes.as_slice()).expect_err("oversized frame must fail");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn read_frame_rejects_truncated_payload() {
    let mut bytes = 2_u32.to_be_bytes().to_vec();
    bytes.push(b'{');
    let error =
        read_frame::<Request>(&mut bytes.as_slice()).expect_err("truncated frame must fail");
    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
}

#[test]
fn request_rejects_unknown_fields() {
    let request =
        format!(r#"{{"protocol":"{PROTOCOL}","request_id":7,"operation":"status","extra":true}}"#);
    let error = serde_json::from_str::<Request>(&request).expect_err("unknown field must fail");
    assert!(error.to_string().contains("unknown field `extra`"));
}

#[test]
fn unix_pair_round_trips_framed_request_and_response() {
    let (mut client, mut server) = UnixStream::pair().expect("unix pair");
    let server_thread = thread::spawn(move || {
        let request: Request = read_frame(&mut server).expect("server reads request");
        assert_eq!(request.operation, Operation::Probe);
        write_frame(&mut server, &response(request.request_id)).expect("server writes response");
    });

    write_frame(
        &mut client,
        &Request {
            protocol: PROTOCOL.into(),
            request_id: 9,
            operation: Operation::Probe,
        },
    )
    .expect("client writes request");
    let reply: Response = read_frame(&mut client).expect("client reads response");
    assert_eq!(reply.request_id, 9);
    server_thread.join().expect("server thread");
}

#[test]
fn call_rejects_response_with_wrong_protocol() {
    let socket = socket_path();
    let listener = UnixListener::bind(&socket).expect("bind listener");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept client");
        let request: Request = read_frame(&mut stream).expect("read request");
        let mut reply = response(request.request_id);
        reply.protocol = "ranger.framework.runtime.v0".into();
        write_frame(&mut stream, &reply).expect("write response");
    });

    let error = call(&socket, Operation::Status, 17, Duration::from_secs(1))
        .expect_err("wrong protocol must fail");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    server.join().expect("server thread");
    std::fs::remove_file(socket).expect("remove socket");
}

#[test]
fn call_rejects_response_with_wrong_request_id() {
    let socket = socket_path();
    let listener = UnixListener::bind(&socket).expect("bind listener");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept client");
        let _: Request = read_frame(&mut stream).expect("read request");
        write_frame(&mut stream, &response(18)).expect("write response");
    });

    let error = call(&socket, Operation::Status, 17, Duration::from_secs(1))
        .expect_err("wrong request ID must fail");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    server.join().expect("server thread");
    std::fs::remove_file(socket).expect("remove socket");
}

#[test]
fn call_times_out_when_unaccepted_backlog_is_full() {
    let socket = socket_path();
    let listener = UnixListener::bind(&socket).expect("bind listener");
    // SAFETY: the descriptor belongs to a live UnixListener and remains valid
    // for the call.
    let result = unsafe { libc::listen(listener.as_raw_fd(), 0) };
    assert_eq!(result, 0, "set listener backlog");
    let _first_client = UnixStream::connect(&socket).expect("fill listen backlog");

    let started = Instant::now();
    let error = call(&socket, Operation::Status, 17, Duration::from_millis(80))
        .expect_err("full listen backlog must time out");
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(1));

    drop(listener);
    std::fs::remove_file(socket).expect("remove socket");
}
