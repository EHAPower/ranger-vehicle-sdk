# Ranger Vehicle SDK schema 输入

本目录保留当前接口提取的机器可读 schema 输入：九个对外 schema 与 `check_registry.yaml` metadata。YAML、`schema_id`、version 或 hash 的存在只表示结构化输入已经保存，不表示 code generation、drift gate、Server compatibility metadata 或 connect-time negotiation 已实现。

迁移期间，`ranger-vehicle-runtime`（原 `ranger-server`）中的同源 schema 仍是生产消费者正在使用的来源；本仓是发布候选快照。切换完成前不得在两个副本分别修改 schema。需要变更时，应先确定唯一 owner、兼容策略、release manifest 影响、Runtime/`ranger-vehicle-apps` 消费者迁移和验证组合。

对 mutating command、安全关键配置、profile、calibration 和 capability 的未知字段、未知 enum、未知 unit、未知 resource、schema mismatch 或 migration 缺失，目标协议仍应 fail fast。telemetry 可以增加字段，但稳定 path、unit、quality、event id 与 reason 语义不得漂移。

`check_registry.yaml` 是原 Server profile checker 使用的 metadata 输入。该 checker 未包含在此仓；文件随接口候选快照保留，不能被误解为本仓已提供 release/profile 验证工具。
