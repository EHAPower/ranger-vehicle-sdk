# SDK 局部规则

本目录是 Ranger Client SDK 边界；`ranger-vehicle-sdk` 是 Client、共享 contracts 和正式 schema 的唯一源码 owner。Runtime / Apps 使用仓内 SDK submodule 固定版本；checker 算法和真实 Server 集成测试留在 Runtime。

- SDK 只使用 Runtime Server API/协议，不打开 CAN、EtherCAT、S.Bus、MAVLink、LiDAR、wheel 或 EHA backend，也不依赖 `daemon`/`hal` 内部模块、SOEM、SocketCAN 或 vendor SDK。
- 不把 Runtime 内部状态机、线程模型或 hardware profile 暴露成客户 API。`schema/` 是机器可读 schema source of truth；DTO、codegen、drift gate 与 negotiation 的实际状态由 schema README 和实现说明，不能把文档/examples 再做稳定 schema。
- 真实动作、maintenance、clear fault 或 emergency stop 示例必须标示前置条件和安全后果；SDK 文档说明 API/SDK 版本、兼容范围和 receipt 与 execution status 的差别。
- 新语言包或发布接口先明确发布方式、依赖、测试入口、版本/兼容性和维护 owner；不把未发布 Runtime 内部字段承诺为稳定合同。
