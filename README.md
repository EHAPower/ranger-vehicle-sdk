<!-- Copyright The ranger Contributors -->

# Ranger Vehicle SDK

本仓是 Ranger 车辆 SDK 的架构基线，当前没有运行构件：没有 Cargo workspace、协议、DTO、schema、示例、测试或常驻进程。

SDK 的目标形态是各车辆 App 在进程内链接的客户端库，经 Runtime 的公开本地 API 或 Unix IPC 请求运行态能力。它不拥有 Cloud 连接、systemd 运维、实时调度、执行器、传感器或其他硬件资源；Runtime 不可用时也不能绕过 Runtime 访问这些资源。

后续只在存在明确 Runtime owner、App 消费者、版本组合和验证路径时建立接口。变异请求必须保留稳定操作身份，结果未知时不自动重试或恢复旧控制权。历史实现通过 Git 的 `release/sop-v1` 追溯，不作为兼容层或代码来源。
