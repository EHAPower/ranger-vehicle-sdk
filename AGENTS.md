# Ranger Vehicle SDK contribution guide

## 主干重构与 SOP 基线

- `main` 正在进行整体架构重构，后续设计与实现迭代在此推进；允许重设接口，不承担旧版本兼容承诺。目标能力仍须以实际实现与验证为准。
- `release/sop-v1` 承接 SOP 维护，后续仅按需修复问题，不直接合入 main 的整体重构；跨仓修复须明确配套版本与验证。
- 2026-09-29 本次 SOP 基线 commit：[`5d6aaa4ebfe36a78fe0200754c72f04a8b4030c3`](https://github.com/EHAPower/ranger-vehicle-sdk/commit/5d6aaa4ebfe36a78fe0200754c72f04a8b4030c3)。这是同步远端并合入本地已有提交后的切分点，不随 main 推进更新。
- 分支名称与基线记录不表示已经完成产品发行或实车验收；组件组合由 `ranger` 集成仓的 gitlink 固定。

**当前阶段：架构重构。** 仓库名称与职责作为设计基线；更名不表示目标能力、接口迁移或产品发行已完成。目标接口允许重设，不承担旧版本兼容承诺；当前行为仍以源码和实际验证为准。

`ranger-vehicle-sdk` 面向 SDK 用户和接口维护者。它只包含 Client library、共享 DTO 和 schema 输入，没有常驻进程。车辆级硬件生命周期与安全裁决仍由 `ranger-vehicle-runtime`（原 `ranger-server`）负责；在接口迁移完成前，它仍是实际生产消费者使用的来源。

## 变更边界

- 不在此仓加入 CAN、EtherCAT、S.Bus、MAVLink、硬件 backend、Server 内部模块或直接执行硬件动作的示例。
- `contracts` 的 envelope、DTO、错误语义和 `sdk/schema` 的 metadata 都是跨仓接口。在迁移完成前，`ranger-vehicle-runtime`（原 `ranger-server`）仍有当前生产消费者使用的同源副本；修改前必须确定唯一 owner、发布顺序、兼容版本和消费者验证，不能让两处独立演进。
- schema 身份、文件 hash 或 YAML 存在不表示 code generation、drift gate 或运行期 negotiation 已实现。不得以文档或 schema 补写未落地的 Runtime 行为。
- `Accepted`、backend committed、动作完成、stop confirmation 和物理安全是不同状态；SDK API、示例和错误处理必须分别表达。
- Client 只能通过 Server API 通信。它不得自动重发非幂等请求、恢复旧控制权或绕过 Server safety gate。

## 验证与文档

- 修改 Rust 代码时运行格式检查、Clippy、测试和 release build；平台边界仍须单独验证。
- 修改 schema、DTO 或 wire bytes 时，同步记录兼容影响和所有已知消费者的验证结论。
- 文档只描述已实现、已验证或明确标记为目标的状态。真实硬件、部署和外部服务操作需要独立授权和相应 SOP。
