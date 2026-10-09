# Ranger Client SDK

当前处于主干架构重构阶段；本仓是 Rust SDK、共享 DTO 和正式 schema 的唯一源码 owner。

本目录是 Ranger 面向客户、上层系统和运维工具的 Client SDK 边界。当前实现从 Runtime `11780dca1518fd9002e2bf8ff7e5179986234027` 迁入；Runtime / Apps 通过仓内 SDK submodule 和 Cargo path 消费本仓唯一来源。SDK 可以独立发布，用于调用 Runtime Server 提供的车辆级 API；它不直接访问传感器、作动器或硬件 backend。

ADR 0003 已固定 SDK / Server API 的参考模式：Ranger 借鉴 Autoware Vehicle Interface / vehicle_cmd_gate 的边界分层，让 SDK consumer 只提交车辆级意图，由 Runtime Server 统一完成 session / resource 校验、command gate、安全裁决、receipt / status / fault 输出和 backend dispatch。SDK 不引入 Autoware 或 ROS 2 runtime，不提供 Autoware topic / message 兼容，也不把 planning、route、trajectory 或 mission 作为 v1 稳定 API。

## 当前实现范围

Rust `ControlClient` 提供 `connect`、`send_motion`、`heartbeat`、`request_mode`、`stop`、`clear_faults`、`query_operation`、`readiness` 和 `reset_eha_fault`。`send_motion`、`stop` 和 lifecycle request 都要求调用方显式传入观察到的 `expected_mode + command_epoch`；SDK 不缓存、猜测或自动重写它们。Server 的 ReadOnly diagnostics 与 suspension DTO/消费链路仍未由 Rust SDK 包装为便捷 API。

本轮 operation DTO、wire v2 和 SDK 调用面已落地。active mode 仍需 Server capability gate；没有 physical E-Stop 输入事实、资源映射、受控 stop deadline、safe-output 或 terminal-confirmation evidence 时，Server 必须保持 `Standby + MotionEnable=Denied`。host 测试不能证明车辆已具备 live mode switch。Connect bootstrap 已对三项 Server API schema
实行 exact version/hash 协商；SDK ReadOnly diagnostics helper 仍未接入。

## Rust 快速入口

`sdk/rust/` 提供 package `ranger-client`，使用同步 Linux Unix domain `SOCK_SEQPACKET` 连接 Runtime Server。以下调用示意要求调用方先提供监听 `/tmp/ranger-sdk-mock.sock` 的无硬件 mock Server；本仓没有可直接启动的 mock 程序或内置 dry-run。参数、mode 和 epoch 仅为测试 fixture，不是部署默认值。`send_motion` 会提交运动请求；真实控制必须先满足 Server profile、操作授权和安全前置条件：

```rust
use std::path::PathBuf;

use ranger_client::{ClientConfig, ControlClient, OperationMode, VehicleMotionCommand};

fn main() -> Result<(), ranger_client::ClientError> {
    let mut client = ControlClient::connect(ClientConfig {
        socket_path: PathBuf::from("/tmp/ranger-sdk-mock.sock"),
        max_frame_bytes: 65_536,
    })?;
    let receipt = client.send_motion(
        OperationMode::Manual,
        42, // 从已确认的 mode transition/readiness 读取，不能本地猜测
        "yard-low-speed.v1",
        VehicleMotionCommand {
            linear_velocity_mps: 0.8,
            yaw_rate_radps: 0.15,
        },
    )?;
    // Accepted 只表示 Server gate 接受，不表示 backend 已执行或车辆已经运动。
    println!("{}: {:?}", receipt.receipt_id, receipt.status);
    Ok(())
}
```

`request_mode(...)` 是唯一 mode 选择入口。它和 `stop(...)`、`clear_faults(...)` 都返回 `OperationReceipt`；`Accepted` 只表示 Server 已接纳 operation。调用方必须以 `query_operation(operation_id)` 读取 `OperationEvent`，等待 terminal phase 和 backend confirmation，随后再读取 `readiness()` 获取新的 mode / epoch。SDK 不自动重连、不自动重发 mutating request，也不把 active-to-active 切换隐藏为回退流程。

Connect 会携带 `command_request`、`receipt_event` 和 `telemetry_diagnostics` 的正式 v2
version/hash manifest。Server 必须逐项返回完全相同的协商结果；任一项缺失、重复、版本不符或 hash 漂移时，SDK 拒绝建立 Control session。

`clear_faults(...)` 是车辆级 clear 请求。它带 mode、epoch、source timestamp 与有效期，但不携带“故障已消失”的调用方断言；只有 adapter 观察到所有 clear blocker 已消失时 Server 才可清除并回到 `Standby + MotionEnable=Denied`。`reset_eha_fault(reason)` 保留为 isolated EHA 域的窄兼容入口，不覆盖全局 E-Stop、不改变 command epoch，也不是停止确认或车辆级 fault clear 的替代。

## 目标职责

- 封装当前同机 Unix `SOCK_SEQPACKET` Ranger Server API transport；LCM、Zenoh、远程网关或其他 Unix adapter 都是未来方案，须先按 ADR 0003 的 RFC / spec 前置约束评估。
- 提供车辆级 request / response / receipt / diagnostics subscription 的稳定类型和错误语义。
- 为 `ranger-cli`、dimos、telemetry、maintenance 工具提供统一调用入口。
- MAVLink 若需要兼容地面站，只能重新实现为只读 telemetry bridge，消费 SDK diagnostics / telemetry；不能通过 SDK 暴露 MAVLink command、mode、parameter write、RC override、stop 或 maintenance 入口，也不能复用旧 PX4 / MAVLink 控制或传感器语义实现。
- 车辆控制 API 同一时间只允许一个 active SDK control session；客户若需要多个控制来源，应先在自己的进程内编排，再通过一个 SDK 实例连接 Ranger Server。第二个 control session 建立请求必须被 Server 直接拒绝为 `SessionAlreadyActive`，SDK 不实现等待、抢占或排队。车辆未处于非运动安全状态时，control session 建立请求必须被拒绝为 `VehicleNotReadyForControlSession`。
- active control session 显式关闭、transport 断开、client 进程退出或 heartbeat 超时后，Server 必须失效该 session、当前 command 和 resource claim；SDK 不保留旧 command，也不把控制权交接给另一个 session。
- 允许多个只读 diagnostics / telemetry client 并发连接；这些 client 只能 query / subscribe，不能发送 mode、command、stop / disable、maintenance action、config write 或其他 mutating request。
- 所有 mutating request 必须来自唯一 active control session，包括 `ModeRequest`、`StopCommand`、`DisableMotionCommand`、`ClearFaults`、`MaintenanceRequest`、`SelfTestRequest`、`ConfigWrite` 和 `ActivateProfile`。
- 控制命令以 `ControlFrame` 提交；一个 frame 必须包含至少一个车辆级 command，可以同时包含 `vehicle.motion`、`vehicle.suspension`、`vehicle.corner[...]` 等多个 command，并按整帧 accept / reject。SDK 便捷 API 例如 `send_motion(...)`、`send_suspension(...)`、`send_corner(...)` 只是构造单 command frame。空 `ControlFrame` 必须由 Server 拒绝为 `EmptyControlFrame`，不能作为 no-op 或 heartbeat 使用。
- stable SDK 控制类型只表达车辆级语义：`VehicleMotionCommand`、有完成语义的 `VehicleMotionAction`、`SuspensionCommand`、`VehicleCornerCommand`、Stop / Disable、车辆级 Maintenance / SelfTest。`VehicleCornerCommand` 按 `front_left`、`front_right`、`rear_left`、`rear_right` 这类车辆几何部位表达抬升、下降、保持、回基准或高度目标，不暴露 wheel motor、EHA、backend axis 或 vendor protocol。
- `VehicleMotionAction` 必须暴露 completion basis。`TimedVelocityAction` 在 `TimedOnly` 下只保证命令持续时间；`DriveDistanceEstimateAction` 只能确认轮端估计里程；真实车体位移需要 Server profile 明确声明 odometry / localization capability，默认不属于 SDK v1 稳定能力。
- 从 Server session handshake 获取心跳周期、心跳超时和 frame 有效期配置，并在 SDK builder / runtime 中按这些配置发送 heartbeat、填充 frame-level `valid_for_ms`；最终校验仍由 Server 执行。SDK 不提供 command 级有效期覆盖；需要不同生命周期的目标时由调用方拆成不同 `ControlFrame`。
- target 越过 Server active profile 硬边界时，Server 必须 reject 整帧，SDK 不把这种结果包装成成功或静默 clamp；合法 target 在执行中被 slew-rate、acceleration、jerk 或 delta-from-current 动态限制 shaping 时，SDK 必须暴露 `Limited` execution status 和 reason。
- SDK 同步调用只返回 Server gate receipt：`Accepted` 或 `Rejected`，并暴露 stable reason、frame id 和 sequence；`Accepted` 只表示 Server gate 允许请求进入控制系统，不表示 backend 已执行或动作已完成。`Committed`、`Limited`、`Observed`、`TimedOut`、`Failed` 通过异步 execution / diagnostics subscription 暴露。普通 motion / suspension / corner 不等待每帧 `Observed`；stop / disable / Estop / maintenance / self-test 必须暴露 backend confirmation、completion、timeout 或 failed reason。
- SDK 可连续发送普通 motion / suspension / corner `ControlFrame`；这些 frame 表示当前控制目标，不是排队任务。Server accepted 新 frame 后覆盖 latest command surface；旧目标不会被排队延迟执行。SDK caller 如需等待 `Committed` / `Observed` 后再发下一帧，可以自行实现该策略，但 Server API 不强制。
- 13S 模块阻抗路径由 Server 对高度与 roll/pitch 合成的四角虚拟平衡位置共用 50 mm/s 限速；同帧有效 motion 的 yaw 正值（左转）另行叠加 `[左前+x, 右前−x, 右后+x, 左后−x] mm（软/中/硬档 x=30/20/15）`，右转相反，零 yaw 或无 motion 时取消偏置。参数由 Server profile 显式配置，SDK 无新增字段；平滑不延迟 Stop 或失效处理。
- `ModeRequest`、`MaintenanceRequest`、`SelfTestRequest`、`ConfigWrite`、`ActivateProfile` 和多步骤流程必须按 operation 暴露 `operation_id`、progress、completion、failure、abort、timeout 和 safe exit 结果；sequence operation 默认 `stop_on_error = true`。
- `ModeRequest` 是唯一能由外部主动选择 operation mode、尤其是进入 active mode 的请求。普通 `ControlFrame` 只携带 `expected_mode + command_epoch` 作为断言，不能隐式切 mode；mode / epoch 不匹配必须由 Server reject，SDK 不在本地重写。Stop、fault、session invalidation 和 operation complete / abort 只能由 Server 强制回到更安全的 `Standby`。
- active-to-active mode 不直切。SDK caller 必须先请求 `Standby`，等待 Server 完成 command / claim 失效、整车 safe output 和全部 backend stop confirmation，再按新 epoch 请求 `Manual / Auto / Maintenance / SelfTest`。SDK 不把这段流程隐藏成自动 fallback，也不在失败后恢复旧 mode 或旧 command。
- 通过 session handshake / capability query 暴露 Server 交叉校验后的有效 capability 和 disabled reason；SDK 不根据 backend 名称、本地配置或调用方类型自行推断 capability。依赖不可用 capability 的 request 必须按 Server 返回的 stable capability / profile reason 暴露给调用方。
- 把 handshake 返回的 `allowed_modes / allowed_operations` 当作冻结的 Server grant，不在本地扩大权限。授权、active profile 或 capability 变化导致 grant / `command_epoch` 失效时，SDK 必须暴露 invalidation 并按 Server 要求重新 handshake，不能沿用旧 grant。
- 暴露 Server 返回的 release capability evidence level 和 self-test / maintenance operation 结果。SDK 可以显示某项能力当前最高证据层级，例如 mock / HIL / bench / static vehicle / field soak 或 L0-L7 自检结果，但不在本地把 mock 通过解释成实车 motion ready。
- 封装通信机械动作，例如 read-only reconnect、query retry、subscription resubscribe、backoff 和幂等 request retry；同时把 `TransportDisconnected`、`Reconnected`、`ServerRestarted`、`ControlInvalidated`、`SessionExpired`、`BackendUnavailable`、`CapabilityChanged`、`FaultLatched`、`SafeOutputActive`、`CommandRejected`、`OperationFailed` 等稳定 lifecycle / recovery event 暴露给调用方。
- 暴露 operator-visible readiness snapshot 和 fault / reason model，包括 `ready_for_control`、`ready_for_motion`、`motion_enabled`、`operation_mode`、`safety_state`、active session、resource claim、E-Stop、safe output、fault latch、compatibility、backend readiness、freshness、clearability、clear blockers 和 recovery hint。
- 暴露 diagnostics typed helper 和只读 field bundle export。SDK 可以提供 `diagnostics().readiness()`、`diagnostics().wheel("FL")`、`diagnostics().subscribe("/safety")`、`diagnostics().export_bundle(...)` 等便捷入口；底层仍映射到 Server 的稳定 diagnostics path。普通 SDK 用户不需要手写 path，但 path 语义必须和 CLI、telemetry、field bundle 一致。
- 对 diagnostics / event / telemetry subscription 暴露 Server recorder 的稳定顺序语义：record 按 Server monotonic receive timestamp + recorder sequence 排序，同时保留 source timestamp 和 clock domain。SDK 不在本地重新拼接散落日志文件，也不自行决定落盘、轮转或容量淘汰策略；这些由 Server `DiagnosticsRecorder` / log store 统一负责。
- ADR 0005 已固定 schema / diagnostics / evidence 的成熟参考裁剪：SDK 采用 Ranger 自有 DTO 和 Server API schema，参考 F Prime / cFS / cFE 的 command / event / telemetry channel / parameter 分层、MAVLink Command 的 ACK / progress / final result 语义、MAVLink Component Metadata 的 schema hash / capability discovery 思路，以及 eha_controller 的结构化 schema 源输入和 drift gate；SDK 不引入这些项目的 runtime、wire protocol、GDS、autocoder、flight mode、MAVLink parameter write 或 WIP Events Interface 作为稳定依赖。
- 将 Server-owned control sensor 的状态作为只读 diagnostics / telemetry 暴露给 SDK consumer，例如 body IMU / attitude source、GPS / time quality、sensor freshness、clock domain 和 quality。SDK consumer 可以据此做上层融合、导航或 operator 展示，但 SDK 不把这些状态包装成可绕过 Server 的控制入口，也不把 GPS / Livox 点云升级为 Ranger 拥有的导航级 localization truth。
- 将 wheel feedback 作为只读 telemetry / diagnostics 暴露给 SDK consumer，包括每轮目标回显、实测速度、timestamp、clock domain、quality、stale reason、backend status、fault、command commit reference 和 stop confirmation。SDK 不把这些反馈包装成 Ranger 拥有的导航级 odometry / pose truth；需要定位融合的调用方自行处理坐标系、轮距、轮径、滑移、时间同步和 stale 策略。
- 将 `clear_faults` 暴露为受控 mutating request；成功后只表示允许回到 `Standby + MotionEnable=Denied(standby)` / readiness restored，不表示恢复旧 mode、旧 command 或旧 motion。
- 维护 SDK version、Server API schema version / hash、command / event / telemetry / parameter schema compatibility 和 release compatibility 说明；SDK DTO 类型必须和 Server handshake 暴露的 schema 匹配。
- `sdk/schema/` 是 Ranger 目标机器可读 schema 的 source of truth。正式 schema 文件落地后，SDK、Server、release manifest、active config checker、diagnostics、field bundle、fixture 和生成物必须从这些结构化源输入派生；[Runtime specs](https://github.com/EHAPower/ranger-vehicle-runtime/tree/main/docs/specs) 负责解释语义和设计理由，不长期承担机器可读合同真值。
- 暴露 release manifest / compatibility 摘要，例如 release id、Server API schema hash、实际运行 TOML 内容身份（包含标定）、hardware profile hash、backend capability schema / runtime capability hash 和 SDK compatibility reason。SDK 不在本地修正错包、错 profile 或错标定；只展示 Server 返回的 stable reason。
- 提供 examples 和集成说明，帮助客户在不理解 Server 内部实现的情况下调用 Ranger。

## 非目标

- 不打开 Ranger Server 的 CAN、EtherCAT、MAVLink、Livox、wheel driver、eha_controller 或其他硬件资源。客户可以在自己的 RC bridge 进程中读取 RC / S.Bus，但该 bridge 对 Ranger 只表现为 SDK consumer。
- 不依赖 `crates/daemon` 内部 module、`crates/hal`、SOEM、SocketCAN、vendor SDK 或具体 backend。
- 不在 SDK 本地判断 backend family、capability schema、profile / calibration compatibility 或 release compatibility；这些由 Server 基于 release metadata、active hardware profile 和 runtime backend capability report 三方交叉校验。
- `MaintenanceRequest` / `SelfTestRequest` 只覆盖车辆级维护 / 自检 action，例如 maintenance readiness、vehicle readiness check、group health check、safe output check、profile / calibration reconfirm；它们可以按维护 profile 绕开 `dimos ready`、`GPS / localization ready`、`Auto readiness` 和普通 motion limit profile。`S.Bus / RC ready` 不是 Ranger Server 默认前置条件；若客户 RC bridge 自己定义 readiness，应由客户编排层处理。Maintenance / SelfTest 不能绕开 active control session、本地或受控授权、审计、physical E-Stop、Server safety state、硬边界、车辆非运动状态、互锁、timeout、abort、safe exit、backend capability、release / profile / calibration compatibility。SDK 不把 `WheelVelocityCommand`、direct actuator、single wheel jog、single EHA jog、backend enable / disable、raw DS402 / PDO / SDO、raw velocity 或 vendor payload 暴露为稳定 API，这些属于 EHA / wheel / backend 级维护工具、受控 bring-up 或离线维护流程。
- 不实现多控制来源的优先级、抢占或资源仲裁；这些属于客户自己的上层控制编排逻辑。
- 不实现车辆级 safety policy，不绕过 Ranger Server 的 safety gate、supervision 或 resource claim；SDK 发起的 mutating request 也必须来自唯一 active control session。
- 不把 session heartbeat timeout、frame default / max valid time 或 safety timeout 写死在 SDK；这些必须来自 Server 受控配置和 session handshake。
- 不隐藏 Server restart、session invalidation、backend degraded、capability mismatch、fault latch、safe output active 或 command invalidation；不自动恢复车辆运动，不自动重发非幂等 mutating request，不替调用方决定是否重新接管控制。
- 不把 `clear_faults` 包装成无条件清零按钮；触发条件未消失、不可软件清除、需要人工检查、profile mismatch 或 unsafe state 时，SDK 必须暴露 Server 的稳定拒绝原因。
- 不让 diagnostics / telemetry subscription 反压 Server RT 路径；慢只读 client 必须由 Server 断开、降级或丢弃旧 telemetry。
- 不把 diagnostics path 或 field bundle export 变成控制入口；它们只能 query / subscribe / export，不能写 mode、command、stop、clear fault、maintenance action 或 config。
- 不默认导出密钥、token、生产隐私数据或无限制 raw trace；field bundle 导出策略必须遵守 Server 返回的权限、脱敏、容量和时间范围限制。
- 不在 SDK 侧实现第二套日志 owner；SDK 只消费 Server recorder 暴露的 query / subscribe / export 结果。

## 建议结构

```text
sdk/
  README.md
  AGENTS.md
  docs/
    README.md
  schema/
    README.md
    command_request.yaml
    receipt_event.yaml
    telemetry_diagnostics.yaml
    platform_profile.yaml
    release_manifest.yaml
    active_config.yaml
    runtime_capability.yaml
    field_bundle.yaml
    check_registry.yaml
    Cargo.toml
  examples/
    README.md
  rust/
    ranger-client
  schema/src/lib.rs
    ranger-schema-sources
```

ADR 0003 已采纳 SDK / Server 架构边界；当前目录包含已实现的 Rust Client、schema 和文档入口。`sdk/schema/` 是目标机器可读 schema 的 source of truth 和发布位置，首批正式 YAML 集合采用 SDK API + release / profile checker 两条线。正式 YAML 已在 `sdk/schema/` 落地；contracts DTO 目前仍手工对齐；三项 Server API schema 的 connect-time exact version/hash negotiation
已实现；Runtime 的 profile checker 测试对照本仓源文本包验证正式 entries 与 contracts identity。codegen 仍待后续规划。车辆接口、mode 和 profile 的目标语义仍分别由对应 spec 约束，不把 YAML 存在等同于目标功能完成。具体语言包、发布流程、transport 选择、权限模型和 schema 生成 / 校验工具需要在 SDK / API 实现计划中再落地。

2026-09-15：可选 `[lidar_heading]` 启用雷达静止零偏校准与重力水平投影。
新 diagnostics `/attitude/level_heading_deg`（相对角度、俯视逆时针正）和
`/attitude/heading_reference_ns`（校准标识）独立于 legacy yaw；旧 consumer 可忽略。
校准标识变化后必须放弃旧绝对航向锚点；启动/重连后车辆需保持静止约 3 s。
字段缺失、陈旧时不能作为运动反馈。配置和实时性边界见 [docs/SDD.md](https://github.com/EHAPower/ranger-vehicle-runtime/blob/main/docs/SDD.md) 的雷达水平相对航向说明。
