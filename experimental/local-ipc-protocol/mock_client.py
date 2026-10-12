#!/usr/bin/env python3
"""实验 mock Runtime 的有界 SOCK_SEQPACKET 客户端；不连接或控制真实车辆。"""

import argparse
import json
import socket
import time

PROTOCOL = "ranger.mock-runtime.seqpacket.v1"
CLOCK_DOMAIN = "linux.boot_monotonic"


CHANNEL_COUNT = 8


def request(sock: socket.socket, packet: dict) -> dict:
    """发送一个完整 packet 并在 socket timeout 内取得对应响应。"""
    encoded = json.dumps(packet, separators=(",", ":")).encode()
    sent = sock.send(encoded)
    if sent != len(encoded):
        raise RuntimeError(f"SOCK_SEQPACKET short send: {sent}/{len(encoded)}")
    return json.loads(sock.recv(4096).decode())


def print_packet(label: str, packet: dict) -> None:
    print(f"{label}: {json.dumps(packet, ensure_ascii=False, sort_keys=True)}")


def absolute_deadline(ttl_ms: int) -> int:
    return time.clock_gettime_ns(time.CLOCK_MONOTONIC) + ttl_ms * 1_000_000


def wait_result(sock: socket.socket, operation_id: int, polls: int, poll_s: float) -> dict:
    """有界查询既有 operation；绝不重投原 mutation 或刷新其 TTL。"""
    latest: dict = {}
    for attempt in range(polls):
        latest = request(sock, {"type": "result", "operation_id": operation_id})
        print_packet(f"result[{operation_id}]#{attempt + 1}", latest)
        state = latest.get("state", {})
        if state.get("status") not in {"accepted", "executing"}:
            return latest
        if attempt + 1 < polls:
            time.sleep(poll_s)
    return latest


def accepted_operation(packet: dict) -> int | None:
    decision = packet.get("decision", {})
    if decision.get("status") != "accepted":
        return None
    return decision["operation_id"]


def is_host_applied(packet: dict) -> bool:
    state = packet.get("state", {})
    return state.get("status") == "completed" and state.get("evidence") == "host_applied"


def main() -> None:
    parser = argparse.ArgumentParser(description="Ranger mock Runtime 实验客户端")
    parser.add_argument("socket_path")
    parser.add_argument("--channels", type=float, nargs=CHANNEL_COUNT, default=[0.0] * CHANNEL_COUNT)
    parser.add_argument("--ttl-ms", type=int, default=500)
    parser.add_argument("--polls", type=int, default=4)
    parser.add_argument("--poll-s", type=float, default=0.02)
    parser.add_argument("--timeout-s", type=float, default=1.0)
    parser.add_argument("--observe-only", action="store_true")
    args = parser.parse_args()
    if args.ttl_ms <= 0 or args.polls <= 0 or args.poll_s < 0 or args.timeout_s <= 0:
        parser.error("ttl、polls、poll-s 和 timeout-s 必须为正（poll-s 可为零）")

    with socket.socket(socket.AF_UNIX, socket.SOCK_SEQPACKET) as sock:
        sock.settimeout(args.timeout_s)
        sock.connect(args.socket_path)
        hello = request(sock, {
            "type": "hello", "protocol": PROTOCOL, "clock_domain": CLOCK_DOMAIN,
            "client_label": "python-mock-fixture",
        })
        print_packet("hello", hello)
        if hello.get("type") != "hello":
            print("handshake was rejected; no control request is sent")
            return
        status = request(sock, {"type": "status"})
        print_packet("status", status)
        if status.get("type") != "status":
            print("status is unavailable; no control request is sent")
            return
        if args.observe_only:
            return

        epoch = status["status"]["epoch"]
        acquire = request(sock, {
            "type": "acquire", "request_id": 1, "expected_epoch": epoch,
            "expires_at_ns": absolute_deadline(args.ttl_ms),
        })
        print_packet("acquire", acquire)
        acquire_operation = accepted_operation(acquire)
        if acquire_operation is None:
            print("acquire was not accepted; no target or stop request is sent")
            return
        acquire_result = wait_result(sock, acquire_operation, args.polls, args.poll_s)
        if not is_host_applied(acquire_result):
            print("control was not host-applied; no target or stop request is sent")
            return

        target = request(sock, {
            "type": "target", "request_id": 2,
            "epoch": epoch,
            "expires_at_ns": absolute_deadline(args.ttl_ms),
            "target": {"fixture_channels": args.channels},
        })
        print_packet("target", target)
        target_operation = accepted_operation(target)
        if target_operation is not None:
            wait_result(sock, target_operation, args.polls, args.poll_s)

        stop = request(sock, {"type": "stop", "request_id": 3, "reason": "python_fixture_complete"})
        print_packet("stop_request", stop)
        stop_operation = accepted_operation(stop)
        if stop_operation is not None:
            print("stop result is host-level only; unknown does not prove physical stop")
            wait_result(sock, stop_operation, args.polls, args.poll_s)


if __name__ == "__main__":
    main()
