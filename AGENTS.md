<!-- Copyright The ranger Contributors -->

# Ranger Vehicle SDK contribution guide

## 基线范围

`ranger-vehicle-sdk` 的目标是为上层车辆控制调用提供统一 SDK。它不定义 `ranger-vehicle-base` 中 Chassis 的本机 socket 协议，也不是访问底盘服务的必经层。车辆模型和控制计算归 `ranger-vehicle-apps` 的上层控制实现；SDK 的具体 API、调用方式及实现组织随实际消费者确定。

当前代码保留的是旧 `Status`/`Probe` Runtime 框架的 DTO、帧编解码和一次性客户端，现有调用者仍依赖它。该实现不是新底盘接口的合同或代码来源，不能据此扩展、复制或兼容旧合同。

不要为占位建立未消费的 schema、假 client 或第二套 DTO。新接口只能在上层控制能力提供方、实际调用方、合同 owner、版本组合和验证路径明确后创建。

## SDK 边界

- SDK 不是 daemon、消息总线、systemd 服务、Cloud client 或硬件资源 owner；不得打开 CAN、EtherCAT、S.Bus、MAVLink、执行器、传感器或其他硬件资源。
- 新底盘 socket 的 wire 合同仅由 `ranger-vehicle-base/chassis/README.md` 的“本地协议”章节 维护。上层控制实现及 Base 内有明确消费者的服务可按该合同访问底盘服务；SDK 不代理或重定义该路径。
- 新接口如包含变异请求，必须有稳定操作身份；超时或传输失败只能表达 `Unknown`，不得自动重连后重发、推断未生效或恢复旧控制权。`Accepted`、主机应用、设备执行、完成、停止确认和物理安全必须分开表达。

## 维护方式

本仓只保留根 `AGENTS.md` 作为治理规则。根 README 是当前职责的唯一说明；代码、合同和文档在实际落地时由唯一 owner 同步创建。

开始工作时检查分支、HEAD、暂存区和工作区，保留无关修改。按改动风险运行相称的格式、构建、测试和跨进程检查。未经用户明确要求不得提交、推送、改写共享历史或使用破坏性 Git 操作。
