<!-- Copyright The ranger Contributors -->

# Ranger Client SDK 内容

本目录保留从 `ranger-vehicle-runtime`（原 `ranger-server`）提取的 RNG1 客户端接口候选快照：Rust package、schema 输入、文档入口和示例约束。它没有 Runtime implementation、profile checker 或硬件访问能力，也不是当前 mock Runtime 的依赖。

在目标产品中，SDK 作为各车辆 App 的进程内库，经 Unix IPC 调用 Runtime；它不作为独立服务部署，也不参与 systemd 保活、Ubuntu + PREEMPT_RT 的线程调度或硬件所有权。Maintenance 只有在 Runtime 在线且相应 API 已实现时，才可通过 SDK 请求运行态配置或诊断；Runtime 不可用时的本地恢复不经 SDK 绕过 Runtime。

## 已实现的 Rust 范围

`rust/` 的 package `ranger-client` 通过同步 Linux Unix domain `SOCK_SEQPACKET` 连接 `ranger-vehicle-runtime`。它实现 control session handshake、motion、stop、heartbeat 和 EHA 域故障复位的 request/receipt transport。

当前 Apps 与 Runtime mock 软件组合使用同仓 `experimental/local-ipc-protocol`，与本目录的 RNG1/bincode 合同不兼容。这里列出的 Rust API 仅描述保留快照，不能据此推断当前 mock 组合已接入 `ranger-client`。

当前公开 API 为 `ControlClient::connect`、`send_motion`、`heartbeat`、`stop` 和 `reset_eha_fault`：

使用示例与执行前置条件见[根 README](../README.md)。

`contracts` 含有 `Resource::VehicleSuspension`，但该 Client 还没有 `send_suspension` 方法，因此 suspension 不能列为当前 Rust SDK 的公开能力。

`Accepted` 只表示 Server gate 接受 request，不表示 backend executed、动作完成、stop confirmation 或车辆物理安全。本 Client 不提供 execution-status 或 diagnostics 查询 API；这些证据须由部署环境的独立 Server 诊断/操作通道取得，不能由 gate receipt 推断。SDK 不自动重连、重发 mutating request 或恢复旧 motion。

当前 `ConnectRequest.schemas` 发送空集合。schema identity、YAML 输入和本地 DTO 都不能说明 runtime schema negotiation 已实现；Server 尚未提供该能力。完整 `ModeRequest`、统一 resource claim、整车 stop confirmation 也仍未实现。

## 边界

- SDK 只能使用 `ranger-vehicle-runtime` API，不打开 CAN、EtherCAT、S.Bus、MAVLink、GR01 或 backend。
- [`crates/contracts`](../crates/contracts/) 包含 DTO 与 RNG1 envelope。它不属于当前 mock 组合；若继续 RNG1 迁移，wire bytes 变更须先确定实际消费者、owner 和发布组合。
- `schema/` 保留 RNG1 的机器可读输入快照。它不是当前 mock Runtime 的 schema 来源；若恢复迁移，两个副本不得独立演进。
- `docs/` 和 `examples/` 只说明 SDK 使用边界，不能替代 Runtime 产品或安全规格。

开发、验证和迁移状态见仓库根目录的 `README.md` 与 `AGENTS.md`。
