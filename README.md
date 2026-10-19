<!-- Copyright The ranger Contributors -->

# Ranger Vehicle SDK

`ranger-vehicle-sdk` 的目标定位是上层车辆控制统一 SDK，为 Remote 与业务 App 提供车辆级调用接口。车辆模型和控制计算归 `ranger-vehicle-apps` 的上层控制实现；SDK 的具体 API、调用方式及实现组织尚待确定。本仓不定义 `ranger-vehicle-base` 中 Chassis 的本机 socket 协议，也不是调用底盘服务的必经层。

底盘 socket 协议由 `ranger-vehicle-base/chassis/README.md` 的“本地协议”章节 唯一维护。上层控制实现及 Base 内有明确消费者的服务可按该合同访问底盘服务；SDK 不代理、重定义或强制这些调用经过自己。

## 保留的旧框架

当前 `0.1.0` 代码仅保留旧 `Status`/`Probe` Runtime 客户端，现有消费者仍固定依赖其 API。对应的旧服务已从 `ranger-vehicle-base` 删除，本轮不恢复该链路；这些代码既不是新底盘接口，也不代表车辆级 SDK 已实现。

旧 `ranger.framework.runtime.v1` 合同使用 4 字节大端长度加 JSON，最大帧为 16,384 字节；`call()` 每次建立一个连接、发送一个请求并读取一个响应，以一个 deadline 覆盖 Linux 非阻塞连接、读写，检查协议版本和 `request_id`，且不自动重试。Linux 可通过 `peer_uid()` 读取 `SO_PEERCRED` 的有效 UID；其他系统明确返回不支持。

SDK 不拥有 Cloud、systemd 运维、实时调度、执行器、传感器或其他硬件资源，也不包含业务角色、lease、任务、配置或硬件后备路径。未来变异请求必须保留稳定操作身份，结果未知时不得重试或恢复旧控制权。

通用 Rust 检查：

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
```

更早的业务实现与 SOP 通过 Git 历史追溯，不作为兼容层或代码来源。
