# SDK Schema

本目录是 Ranger 目标机器可读 schema 的 source of truth。它承接 ADR 0005、[docs/specs/ranger-vehicle-interface.md](https://github.com/EHAPower/ranger-vehicle-runtime/blob/main/docs/specs/ranger-vehicle-interface.md) 和 [docs/specs/ranger-platform-profile.md](https://github.com/EHAPower/ranger-vehicle-runtime/blob/main/docs/specs/ranger-platform-profile.md) 的已决原则；已完成计划的演进来源见 [docs/history.md](https://github.com/EHAPower/ranger-vehicle-runtime/blob/main/docs/history.md)。

## 范围

正式集合分为三份 Server API 描述、四份 release/profile checker 输入定义和一份 checker 证据输出定义。这些 YAML 描述数据结构，不是逐车填写的参数文件。`parameter_config.yaml` 已退出：现有 Runtime 没有参数 API、参数解析器或配置写入口；`contracts::SchemaId::ParameterConfig` 仅保留为未协商的 wire 预留，不能据此推断存在配置能力。

| 文件 | 范围 |
|------|------|
| `command_request.yaml` | Server API command / request、control frame、operation request |
| `receipt_event.yaml` | gate receipt、execution status、structured event、fault / reason code |
| `telemetry_diagnostics.yaml` | telemetry channel、diagnostics path、readiness snapshot 摘要 |
| `platform_profile.yaml` | compute / OS / RT、target facts、power / thermal envelope、bus topology、sensor / time sync、backend mapping、evidence |
| `release_manifest.yaml` | release bundle metadata、schema hash、supported profile / capability |
| `active_config.yaml` | active release / profile 与实际启动 TOML 的精确字节 hash |
| `runtime_capability.yaml` | backend capability report、schema version、instance binding、freshness、safe output、stop confirmation、fault mapping 和 declared absent / optional / degraded capability 状态 |
| `field_bundle.yaml` | checker readiness/replay record、evidence 与 scoped ignored checks；不定义 bundle export、recorder 或脱敏链 |

这些 schema 是 SDK、Server、release checker、diagnostics、field bundle、回滚和测试 fixture 共享的机器可检查契约。`docs/` 中的 spec 负责解释背景、语义和设计理由；`sdk/schema/` 负责保存可生成、可校验、可计算 hash 的结构化源输入。

`check_registry.yaml` 是 checker metadata source，不计入八份 schema aggregate。它维护稳定 check id、severity、所属 operation、是否 `ignorable_for_commissioning`、reason code 和输出字段映射，并由 Runtime checker 的 build step 生成 Rust 静态 registry 与 hash 常量；运行期不再重复解析同一文件。`ranger-check --list-checks --json` 从生成后的 registry 输出可发现清单。

## 源文本包与消费者

本仓是八份正式 schema 和 registry 的唯一源码 owner。无依赖 Rust 包
`ranger-schema-sources` 提供 `FORMAL_SCHEMA_SOURCES: [(&str, &str); 8]` 和
`CHECK_REGISTRY_SOURCE: &str`，通过 `include_str!` 打包所有 YAML 原始文本。
包中没有 YAML parser、hash 或 checker 算法；Runtime `profile_check` 消费同一包，
负责构建时 registry 生成、运行期 checker 和 schema identity drift 验证。
Runtime / Apps 通过 `deps/ranger-vehicle-sdk` gitlink 与 Cargo path 固定输入版本，
集成仓的直接 SDK gitlink 必须一致。移除的 `parameter_config.yaml` 不属于正式集合。

## 当前状态

八份正式 YAML schema 已落地。Server API 三份 schema 已升级为 v2：`command_request.yaml` 固定 `expected_mode + command_epoch` assertion 与 Mode/Clear 请求，`receipt_event.yaml` 固定 operation receipt/event 和 backend stop terminal evidence，`telemetry_diagnostics.yaml` 固定 operation/safety/readiness 查询。YAML 不自动生成 DTO；WP_VER=2 的 SDK/Server bootstrap 对三项 Server API schema
实行 exact version/hash 协商，缺失或漂移不能建立 Control session。release/profile checker 的 JSON 输入与实际 `config/*.toml` 通过 active config 的 `runtime_config.sha256` 绑定；hash 是启动选用 TOML 的精确字节摘要，包含注释和空白。

每辆车只选择一份完整 `ranger.toml` 作为运行参数来源；当前 daemon startup gate 另外读取 release manifest、active config、platform profile、runtime capability 和 target facts 共五份 JSON。target facts 的结构定义在 `platform_profile.yaml` 中。部署环境文件只保存这些输入的路径。真实发布输入生成与 live target facts collector 尚未闭合，因此当前还不是只提供一个文件即可启动的交付流程。

车辆级运行模式的人读契约由 [docs/specs/ranger-mode-supervisor.md](https://github.com/EHAPower/ranger-vehicle-runtime/blob/main/docs/specs/ranger-mode-supervisor.md) 收敛。v2 已同步 schema、`contracts` DTO、wire v2、Server adapter、Rust SDK、aggregate hash fixture 与测试；它仍要求 runtime capability 给出 physical E-Stop、资源映射、safe output 和 terminal stop confirmation 的证据。缺失任一能力必须按 stable reason 保持 motion denied，不能用 schema 或 host 测试声明 live mode switch 已就绪。

`check_registry.yaml` 提供 registry drift check、commissioning ignore allowlist 和 list-checks JSON fixture。改变 check id、severity、operation、reason code、输出字段或 ignore 规则会改变 `check_registry_hash`；仅改 description 不会改变它。

[Runtime `crates/profile_check`](https://github.com/EHAPower/ranger-vehicle-runtime/tree/main/crates/profile_check) 是 schema/hash、release/profile/config/capability 输入归一化、checker decision 和 `ranger-check` 的边界；`msgs`、`control`、`hal` 和 `daemon` 不拥有 schema hash、check id 或 checker decision 规则。

人读设计真值仍是：

- [docs/specs/ranger-vehicle-interface.md](https://github.com/EHAPower/ranger-vehicle-runtime/blob/main/docs/specs/ranger-vehicle-interface.md)
- [docs/specs/ranger-mode-supervisor.md](https://github.com/EHAPower/ranger-vehicle-runtime/blob/main/docs/specs/ranger-mode-supervisor.md)
- [docs/specs/ranger-platform-profile.md](https://github.com/EHAPower/ranger-vehicle-runtime/blob/main/docs/specs/ranger-platform-profile.md)
- [docs/specs/ranger-commissioning-evidence.md](https://github.com/EHAPower/ranger-vehicle-runtime/blob/main/docs/specs/ranger-commissioning-evidence.md)
- [docs/adr/0005-adopt-schema-diagnostics-evidence-model.md](https://github.com/EHAPower/ranger-vehicle-runtime/blob/main/docs/adr/0005-adopt-schema-diagnostics-evidence-model.md)

当前 `ranger-check` 已支持 `--json`、人读渲染、`--list-checks --json`、CLI 参数错误结果、离线 / fixture `--target-facts <path>` 输入、scoped `--ignore-check <check-id>`、release manifest / active config / profile / runtime capability JSON fixture 输入与 `--config <TOML>` 内容身份校验，并在同一 JSON result 中输出 `readiness` snapshot 与 `field_bundle` replay record。`deploy/service.sh` 和 `deploy/setup-env.sh` 已在 install / switch / daemon start 前调用 `ranger-check --json` 并按 exit code 阻止后续副作用；`ranger-cli --daemon` 也会在打开硬件链路前复用同一个 checker library 并输出 structured checker result。live target facts collector 仍未接入。

## Telemetry diagnostics P0 path 契约

`telemetry_diagnostics.yaml` 同时承载 Ranger Server diagnostics 和 Ranger-monitor 只读展示所需的 P0 path catalog。Ranger daemon 是所有 P0 diagnostics 数据计算 owner，负责完成单位归一化、EHA 角点顺序映射、差压力计算、IMU 单位换算、freshness 和 `DataQuality` 判定。Ranger-monitor 只消费 `DiagnosticsSnapshot` 中的值、`timestamp_ns` 和 `quality`，不做领域计算。

P0 只冻结 pull 模型：ReadOnly client 发送 `DiagnosticsQuery`，daemon 返回 `DiagnosticsSnapshot`。server-push、参数热加载、配置写入和服务重启不属于 P0；写配置和重启服务属于 maintenance capability，不能挂在 ReadOnly diagnostics 权限下。

P0 path 的索引规则固定如下：

| 索引 | 语义 |
|------|------|
| `eha[i]` | `EHA0=LF`、`EHA1=RF`、`EHA2=RR`、`EHA3=LR` |
| `pressure[j]` | 逻辑压力通道 `P0..P7`，每个 EHA 两路，映射由运行 TOML 的 `[pressure_can].channel_map` 进入 daemon 后归一化 |

`/actuator/eha[i]/position_m` 当前语义是由拉线位移换算得到的 EHA 位置反馈，来源为 `DrawWireArray.displacements[i] * 0.001`，不是内部推杆编码器反馈。后续若接入内部编码器，必须新增 path，例如 `/actuator/eha[i]/encoder_m`，不得改变既有 path 语义。

## 规则

- schema 文件必须有稳定 `schema_version`、兼容策略和 hash 计算方式。
- 单文件 schema 与 registry 使用 `semantic_projection_v2`：解析 YAML 后，仅纳入其固定 `semantic_inputs`，递归排序 map、保留 array 顺序，并排除任何 `description`。`semantic_inputs` 必须包含所有必需语义且不能选择不存在字段。说明文字、标题、owner、状态、注释和格式不改变 hash；`required`、type、unit、enum、items、compatibility、schema version、registry severity/operation/reason/ignore 规则改变则必须改变 hash。对应测试覆盖这些边界。
- release aggregate 使用 `ranger.sdk_schema_set.v2`，只由上述八份正式 schema 的文件名、schema version、单文件 hash 和兼容策略按稳定顺序计算。它与旧九文件/v1 canonical identity 不兼容，必须随 release manifest 一同更新。
- mutating command、safety-critical config、profile、calibration 和 capability schema 默认 strict；未知字段、未知 enum、未知 unit、未知 resource、schema mismatch 或 migration 缺失不能隐式兼容。
- 安全阈值和配置语义由实际运行 TOML 的拥有者定义；active config 只绑定启动选用的完整 TOML 字节，不复制参数、标定或 field override 模型。
- telemetry / diagnostics 可以兼容新增字段，但 stable path、unit、quality、event id 和 reason 语义不能漂移。
- 生成物不能成为唯一真值；必须能从本目录的 schema 源输入重新生成。
- check id registry 的真值源是 `check_registry.yaml`。Rust checker registry、CLI help / `--list-checks --json` 和 fixture 必须由该 YAML 或其生成物派生；新增可忽略项必须显式标记 `ignorable_for_commissioning=true`。`check_registry_hash` 进入 release manifest、checker output、diagnostics 和 field bundle，用于审计 checker 行为漂移。
- 后续 generator、fixture、compatibility metadata 和 drift gate 应参考 eha_controller 的 schema 源输入与检查方式，但不复制其协议领域。
