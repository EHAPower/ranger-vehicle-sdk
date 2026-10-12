# `ranger-local-ipc-protocol`

这是 Ranger Runtime 重写期间的**实验性 mock 软件运行时**本地协议合同。它只在 Linux Unix `SOCK_SEQPACKET` 上使用 JSON packet；每个 packet 恰好是一条 JSON 消息。协议标识为 `ranger.mock-runtime.seqpacket.v1`，与 SDK 旧 snapshot 中的 RNG1/bincode 合同完全分离，二者不兼容。

## 范围

合同只表达固定数量 fixture 通道的 mock 目标、会话、接纳、状态和有身份的终态结果。`MockTarget` 中的数值是测试 fixture 通道值，不表示车辆运动学、设备命令、硬件执行或物理安全。Server 在握手中显式声明 `simulated: true` 与 `real_vehicle_capability: "unavailable"`。

客户端必须使用 Linux boot monotonic 时钟域发送 `expires_at_ns`。该值是绝对截止时间；传输和排队会消耗它，Server 不会刷新 TTL。`accepted` 只表示进入 Runtime 的定义处理路径，最终结果须按 `operation_id` 查询，并且只允许原会话读取。

结果保存是有界的。若 Server 已知 `operation_id` 属于当前会话、但 Runtime 的终态记录已被淘汰，返回 `result_unknown`；这不表示操作未发生、失败或已经安全停止，也不允许客户端自动重发原请求。

同一 boot/session 内，保留窗口中的同一 `request_id` 只有在完整消息内容相同才返回最初的接纳记录；内容不同会被拒绝，且不会重新提交 Runtime。记录被有界淘汰后，Server 仍按会话内单调 request ID 水位拒绝旧 ID，并把其结果视为未知；客户端不得借此重放副作用请求。

## 边界

传输层以固定 packet 上限、连接上限与每连接待写上限运行；具体数值由 Runtime Server 配置。非法、截断、未知版本或超限 packet 会被拒绝。Stop 由 Server 直接提交给 Runtime 的独立停止路径，不经过普通目标路径。断开控制连接会撤销该会话。

本 crate 不包含真实硬件、自动重试、自动重连、车辆运动计算或安全判定。

## Python 演示客户端

`mock_client.py` 仅连接已经运行的 mock Runtime。它使用 `CLOCK_MONOTONIC` 生成每个新请求
自己的绝对 deadline，先从 `Status` 读取 epoch，所有 `Result` 查询都有固定次数与 socket
timeout；它不会重投 mutation 或刷新原请求 TTL。

在开发容器中，先启动 mock Runtime，再在另一个终端执行：

```bash
docker exec -it -w /workspace/EHAPower/ranger-vehicle-runtime devbox \
  python3 sdk/experimental/local-ipc-protocol/mock_client.py \
  /run/ranger-vehicle-runtime.sock
```

使用 `--observe-only` 只完成握手并打印 Runtime 状态，不申请控制权或发送 fixture 目标。普通运行也会
因为 mock Runtime 的故障锁存、epoch 变化、已过期 deadline 或控制权条件而被拒绝；这表示
软件状态不允许该请求，并不是设备或车辆故障诊断结论。只有 acquire 的结果明确为
`completed/host_applied` 时，客户端才继续发送 fixture target。Stop 的接纳只表示主机已收到
停止请求；`result_unknown` 和任何软件响应都不证明物理停止或设备执行。

## 验证

实验协议是独立 workspace；从 Runtime 根目录运行：

```bash
docker exec -w /workspace/EHAPower/ranger-vehicle-runtime devbox \
  cargo test --locked --manifest-path sdk/experimental/local-ipc-protocol/Cargo.toml
```

该命令只检查实验协议，不构建或声称兼容旧 SDK workspace。
