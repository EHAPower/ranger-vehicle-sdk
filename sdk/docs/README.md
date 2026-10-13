<!-- Copyright The ranger Contributors -->

# SDK 文档

本目录保存保留的 RNG1 候选快照说明。独立仓只保留 Rust Client、`contracts` 和 schema 输入；Runtime implementation、profile checker、部署脚本和车辆级设计文档不随这份快照迁移。

目标架构中，SDK 是车辆 App 进程内的 Runtime 客户端；Maintenance 的在线配置和诊断也须经它的已实现接口。systemd 运维、Ubuntu + PREEMPT_RT 线程配置和执行器资源归属不属于 SDK 文档或合同。Runtime 不可用时，Maintenance 的本地恢复不能借 SDK 取得硬件访问。

当前 Rust API、错误语义和 transport 实现在 `../rust/`，机器可读输入在 `../schema/`。它们不属于当前使用 `experimental/local-ipc-protocol` 的 mock 软件组合。只有明确恢复 RNG1 的消费者迁移时，才补齐 API、兼容性、权限和发布说明；在 Runtime 实际支持前，不得把计划中的 `ModeRequest`、ReadOnly/diagnostics client、schema negotiation 或整车 stop confirmation 写成稳定 SDK 合同。
