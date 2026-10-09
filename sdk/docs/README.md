# SDK 文档

本目录保存接口迁移期间仍适用的说明。独立仓只保留 Rust Client、`contracts` 和 schema 输入；Runtime implementation、profile checker、部署脚本和车辆级设计文档仍在 `ranger-vehicle-runtime`（原 `ranger-server`），不能在此目录假定它们已随接口迁移。

当前 Rust API、错误语义和 transport 实现在 `../rust/`，机器可读输入在 `../schema/`。完整 API、兼容性、权限和发布说明应随跨仓消费者切换逐步补齐；在 Server 实际支持前，不得把计划中的 `ModeRequest`、schema negotiation 或整车 stop confirmation 写成稳定 SDK 合同。
