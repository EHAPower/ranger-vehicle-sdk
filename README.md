<!-- Copyright The ranger Contributors -->

# Ranger Vehicle SDK

本仓提供 `ranger-vehicle-sdk` Rust library package（`0.1.0`，edition 2024），是各车辆 App 在进程内链接的 Runtime 本地客户端库。当前唯一公开合同是 `ranger.framework.runtime.v1`：通过 Unix stream 发送一次 `status` 或 `probe` 请求，并接收 Runtime 状态。

帧格式是 4 字节大端长度加 JSON，最大帧为 16,384 字节。`call()` 每次建立一个连接、发送一个请求并读取一个响应；它以一个 deadline 覆盖 Linux 非阻塞连接、读写，检查协议版本和 `request_id`，并且不自动重试。Linux 可通过 `peer_uid()` 读取 `SO_PEERCRED` 的有效 UID；其他系统明确返回不支持。

SDK 不拥有 Cloud、systemd 运维、实时调度、执行器、传感器或其他硬件资源，也不包含业务角色、lease、任务、配置或硬件后备路径。Runtime 不可用时不得绕过 Runtime；未来变异请求必须保留稳定操作身份，结果未知时不得重试或恢复旧控制权。

通用 Rust 检查：

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
```

历史实现通过 Git 的 `release/sop-v1` 追溯，不作为兼容层或代码来源。
