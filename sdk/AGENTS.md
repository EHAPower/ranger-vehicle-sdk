<!-- Copyright The ranger Contributors -->

# SDK 目录规则

本目录是 Ranger Client SDK 的接口边界。它面向客户、上层系统和运维工具，只包含
Client library、共享 contracts、schema 输入和 SDK 文档；不要引入 Server 内部实现、
硬件访问或真实硬件动作。

## 迁移状态

本目录是从 `ranger-vehicle-runtime`（原 `ranger-server`）提取的 RNG1 候选快照。
当前 Apps 与 Runtime mock 软件组合使用上层 `experimental/local-ipc-protocol`，并不消费本目录的
`contracts`、schema 或 `ranger-client`。本目录不是当前 mock 合同的 owner，也不能与实验协议混用。
若恢复 RNG1 迁移，变更前必须先确定 owner、固定版本、消费者顺序与验证证据；不得让并行副本独立演进。

## SDK 边界

- SDK 只能调用 `ranger-vehicle-runtime` API 或协议，不直接打开 CAN、EtherCAT、S.Bus、MAVLink、
  Livox、wheel driver、EHA backend 或其他硬件资源。
- SDK 不依赖 Server 内部 module、SOEM、SocketCAN、vendor SDK 或具体 backend。
- schema 输入不等于 DTO code generation、drift gate 或实际 schema negotiation。不得把
  YAML、hash 或设计目标当成已实现运行期能力。
- 示例默认无硬件或只读；需要 mock/dry-run 时应说明由谁提供，本仓无内置实现。任何真实动作、maintenance、clear fault 或
  emergency stop 示例都必须明确前置条件和安全后果。
- 文档必须区分 gate receipt、backend execution、completion、stop confirmation 和
  物理安全；`Accepted` 只表示请求被 Server gate 接纳。
