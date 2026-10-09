# Ranger Vehicle SDK

## 主干重构与 SOP 基线

- `main` 正在进行整体架构重构，后续设计与实现迭代在此推进；允许重设接口，不承担旧版本兼容承诺。目标能力仍须以实际实现与验证为准。
- `release/sop-v1` 承接 SOP 维护，后续仅按需修复问题，不直接合入 main 的整体重构；跨仓修复须明确配套版本与验证。
- 2026-09-29 本次 SOP 基线 commit：[`5d6aaa4ebfe36a78fe0200754c72f04a8b4030c3`](https://github.com/EHAPower/ranger-vehicle-sdk/commit/5d6aaa4ebfe36a78fe0200754c72f04a8b4030c3)。这是同步远端并合入本地已有提交后的切分点，不随 main 推进更新。
- 分支名称与基线记录不表示已经完成产品发行或实车验收；组件组合由 `ranger` 集成仓的 gitlink 固定。

**当前阶段：架构重构。** 仓库名称与职责作为设计基线；更名不表示目标能力、接口迁移或产品发行已完成。目标接口允许重设，不承担旧版本兼容承诺；当前行为仍以源码和实际验证为准。

规范仓库地址：[EHAPower/ranger-vehicle-sdk](https://github.com/EHAPower/ranger-vehicle-sdk)。克隆目录为 `ranger-vehicle-sdk`。crate 名、socket 路径和 wire 合同不因仓库更名改变。

`ranger-vehicle-sdk` 是 Ranger 的客户端接口仓库，也是无常驻进程的库仓。当前内容从 `ranger-vehicle-runtime`（原 `ranger-server`）的 `a691ce1a999314eae5634876786de182915509cb` 提取，包含 Rust Client、共享 `contracts` 和机器可读 schema 输入。它不包含 Runtime 实现、硬件 backend、控制算法或部署配置。

这是一次接口提取的发布候选快照，尚不是协议 owner 已迁移完成的证明。迁移期间，`ranger-vehicle-runtime`（原 `ranger-server`）中的 `crates/contracts` 仍是当前生产消费者使用的来源；在 `ranger-vehicle-apps`、Runtime 和发布流程切换到此仓固定版本前，不得在两个位置独立演进 DTO、schema 或 wire bytes。任何协议改动必须先确定唯一 owner、迁移顺序、兼容策略和验证组合。

## 当前可用接口

- `sdk/rust` 的 `ranger-client`：同步 Linux Unix domain `SOCK_SEQPACKET` Client。
- `crates/contracts`：RNG1 transport envelope、DTO 和 schema 身份类型。
- `sdk/schema`：schema 源输入快照，包括九个对外 schema 和 checker registry metadata。

当前 `ranger-client` 的公开方法是 `ControlClient::connect`、`send_motion`、
`heartbeat`、`stop` 和 `reset_eha_fault`。`contracts` 已定义
`Resource::VehicleSuspension`，但 Rust Client 还没有 `send_suspension` 方法；
不得把 DTO 枚举当成已交付的 suspension SDK API。

下列片段是 API 调用示意，不是可直接运行的程序。演练前须由调用方自行提供监听指定 socket 的无硬件 mock Server；本仓未提供该程序或内置 dry-run。参数仅是测试 fixture，不是部署默认值。`send_motion` 会提交运动请求，不得将该片段直接指向真实车辆端点；实际控制须先满足对应 Server profile 和操作前置条件。

```rust
use std::path::PathBuf;

use ranger_client::{ClientConfig, ControlClient, VehicleMotionCommand};

let mut client = ControlClient::connect(ClientConfig {
    socket_path: PathBuf::from("/tmp/ranger-sdk-mock.sock"),
    max_frame_bytes: 65_536,
})?;
let receipt = client.send_motion(
    "yard-low-speed.v1",
    VehicleMotionCommand {
        linear_velocity_mps: 0.8,
        yaw_rate_radps: 0.15,
    },
)?;
println!("{}: {:?}", receipt.receipt_id, receipt.status);
# Ok::<(), ranger_client::ClientError>(())
```

Client 只通过 `ranger-vehicle-runtime` API 请求 control session、motion、stop 和 EHA 域故障复位；它不会打开 CAN、EtherCAT、S.Bus、MAVLink 或任何 backend。`contracts` 保留 diagnostics DTO，但当前 Rust Client 尚未提供 ReadOnly/diagnostics client。当前 `ConnectRequest.schemas` 发送空集合，Runtime 尚未实现 schema negotiation；schema 文件与 `SchemaId` 的存在不能说明运行时已协商兼容。

`GateReceipt::Accepted` 仅表示 Server gate 接受请求。它不表示 backend 已执行、停止已被 backend 确认，或车辆已处于物理安全状态。当前 Client 只返回 gate receipt，没有 execution-status 或 diagnostics 查询 API；动作结果及停止证据须由部署环境的独立 Server 诊断/操作通道取得，不能由本 SDK receipt 推断。SDK 不自动重连、重发 mutating request 或恢复运动。

当前接口尚未实现完整车辆级 `ModeRequest`、统一 resource claim、整车 stop confirmation 或 schema negotiation。这些能力不能由 SDK 文档或 schema 文件替代。

## 目录

```text
crates/contracts/  共享 DTO、RNG1 envelope 和 schema 身份类型
sdk/rust/          ranger-client Rust package
sdk/schema/        机器可读 schema 输入
sdk/docs/          接口边界与迁移说明
sdk/examples/      SDK 使用示例约束
```

## 本地验证

在具备 Rust Linux 构建环境中执行：

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo build --locked --release
```

`ranger-client` 依赖 Linux Unix domain `SOCK_SEQPACKET`；构建通过不证明目标 Server、profile 或硬件已经验证。

## 迁移边界

此仓添加到 `ranger` 集成仓只固定一次接口候选版本，不会自动让现有 path dependency 切换。下一步应先让 `ranger-vehicle-apps` 改为固定的 `ranger-vehicle-sdk` 依赖并验证 wire 兼容，再让 `ranger-vehicle-runtime` 消费同一版本，最后删除其重复来源。迁移完成前，Runtime 和 Apps 的实际发布组合仍须以各自仓库的锁定 revision 为准。
