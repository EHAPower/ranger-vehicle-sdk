<!-- Copyright The ranger Contributors -->

# Ranger Vehicle SDK contribution guide

## 基线范围

本仓是无运行构件的 SDK 架构基线。当前没有 Cargo workspace、协议、DTO、schema、示例、测试或常驻进程。历史实现仅可通过 Git 的 `release/sop-v1` 与相关提交追溯，不能恢复、复制或兼容旧合同。

不要为占位建立空 crate、未消费的 schema、假 client 或第二套 DTO。新接口只能在 Runtime owner、实际 App 消费者、版本组合和验证路径明确后创建。

## SDK 边界

- SDK 是各车辆 App 进程内链接的客户端库，不是 daemon、消息总线、systemd 服务、Cloud client 或硬件资源 owner。
- SDK 只能通过 Runtime 的公开本地 API 或协议通信；不得打开 CAN、EtherCAT、S.Bus、MAVLink、执行器、传感器或其他硬件资源。
- Runtime 不可用时，SDK 不得绕过它访问硬件、恢复旧会话、lease、epoch 或动作。
- 未来 mutating request 必须有稳定操作身份；超时或传输失败只能表达 `Unknown`，不得自动重连后重发、推断未生效或恢复旧控制权。
- `Accepted`、主机应用、设备执行、完成、停止确认和物理安全必须分开表达。

## 维护方式

本仓只保留根 `AGENTS.md` 作为治理规则。根 README 是当前职责的唯一说明；代码、合同和文档在实际落地时由唯一 owner 同步创建。

开始工作时检查分支、HEAD、暂存区和工作区，保留无关修改。使用 `devbox` 进行开发和验证。未经用户明确要求不得提交、推送、改写共享历史或使用破坏性 Git 操作。
