# Ranger Vehicle SDK

## 主干重构与 SOP 基线

- `main` 正在进行整体架构重构，后续设计与实现迭代在此推进；允许重设接口，不承担旧版本兼容承诺。目标能力仍须以实际实现与验证为准。
- `release/sop-v1` 承接 SOP 维护，后续仅按需修复问题，不直接合入 main 的整体重构；跨仓修复须明确配套版本与验证。
- 2026-09-29 本次 SOP 基线 commit：[`5d6aaa4ebfe36a78fe0200754c72f04a8b4030c3`](https://github.com/EHAPower/ranger-vehicle-sdk/commit/5d6aaa4ebfe36a78fe0200754c72f04a8b4030c3)。这是同步远端并合入本地已有提交后的切分点，不随 main 推进更新。
- 分支名称与基线记录不表示已经完成产品发行或实车验收；组件组合由 `ranger` 集成仓的 gitlink 固定。

**当前阶段：架构重构。** 仓库名称与职责作为设计基线；更名不表示目标能力、接口迁移或产品发行已完成。目标接口允许重设，不承担旧版本兼容承诺；当前行为仍以源码和实际验证为准。

规范仓库地址：[EHAPower/ranger-vehicle-sdk](https://github.com/EHAPower/ranger-vehicle-sdk)。本仓是 Ranger Rust Client、共享 `contracts` DTO、wire envelope、八份正式 schema 与 checker registry 的唯一源码 owner，没有常驻进程、硬件 backend 或 checker 算法。

当前内容从 Runtime `11780dca1518fd9002e2bf8ff7e5179986234027` 迁入，保留 wire v2 和正式 schema 身份。[来源与归属](SOURCE_PROVENANCE.md)记录首次提取、当前迁移和验证归属。运行时与车辆级安全策略仍由 [ranger-vehicle-runtime](https://github.com/EHAPower/ranger-vehicle-runtime) 持有。

## 当前接口

- `sdk/rust` 的 `ranger-client`：同步 Linux Unix domain `SOCK_SEQPACKET` Control Client，提供 `connect`、`send_motion`、`heartbeat`、`request_mode`、`stop`、`clear_faults`、`query_operation`、`readiness` 和 `reset_eha_fault`。
- `crates/contracts`：RNG1 wire v2 envelope、DTO 和严格 schema 身份；当前 DTO 手工对齐 YAML，自动 codegen 尚未实现。
- `sdk/schema` 的 `ranger-schema-sources`：无依赖原始文本包，公开八份正式 YAML 和 registry，供 Runtime 的 build script、checker 和测试使用。

Connect 对 `command_request`、`receipt_event`、`telemetry_diagnostics` 的正式 v2 version/hash 执行精确协商；缺失、重复或漂移会拒绝 Control session。motion、stop 和生命周期请求要求显式传入 Server 已观察到的 `expected_mode + command_epoch`，SDK 不缓存、猜测或自动重写它们。

`GateReceipt::Accepted` 和 `OperationReceipt::Accepted` 只证明 Server 接纳请求。operation 通过 `query_operation(operation_id)` 取得 terminal phase 和 backend confirmation，再以 `readiness()` 读取 mode / epoch；SDK receipt 不证明物理安全。physical E-Stop、safe-output 或 terminal-confirmation 能力缺失时，Runtime 必须保持 motion denied。SDK 不自动重连、重发 mutating request 或恢复旧动作。

Rust Client 尚未提供 ReadOnly diagnostics 或 suspension 便捷 API；DTO 存在不表示调用能力已交付。完整调用边界见 [SDK 说明](sdk/README.md)，正式输入和 hash 规则见 [schema 说明](sdk/schema/README.md)。

## 固定版本接入

Runtime 和 Apps 在各自仓库的 `deps/ranger-vehicle-sdk` 使用 Git submodule，并以 Cargo path 引用本仓 package；gitlink 是源码版本真值，Cargo.lock 仍提交。集成仓 `ranger` 的直接 SDK gitlink 必须与两个消费者的嵌套 gitlink 一致。初始化使用 `git submodule update --init -- deps/ranger-vehicle-sdk`，不使用 `--remote`。

```toml
[dependencies]
contracts = { path = "deps/ranger-vehicle-sdk/crates/contracts" }
ranger-client = { path = "deps/ranger-vehicle-sdk/sdk/rust" }
ranger-schema-sources = { path = "deps/ranger-vehicle-sdk/sdk/schema" }
```

公共 DTO、schema 或 wire 变更必须在本仓维护，随后验证 Runtime / Apps 并更新组合 gitlink；消费者不维护独立副本。SDK 的 socket mock、DTO 和 codec 测试留在本仓；真实 Server 集成与正式 schema identity drift 测试留在 Runtime，SDK 不反向依赖 Server/core/checker。

## 本地验证

在 Linux Rust 开发环境的 SDK 仓库根目录执行：

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo build --locked --workspace --release
cargo package --locked -p ranger-schema-sources --list --allow-dirty
```

`ranger-client` 依赖 Linux Unix domain `SOCK_SEQPACKET`。软件测试与构建通过只证明软件组合，不证明产品发行、目标 Server profile、backend stop confirmation 或实车验收。
