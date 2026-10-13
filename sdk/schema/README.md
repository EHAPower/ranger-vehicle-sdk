<!-- Copyright The ranger Contributors -->

# Ranger Vehicle SDK schema 输入

本目录保留 RNG1 候选快照的机器可读 schema 输入：九个对外 schema 与 `check_registry.yaml` metadata。它们不属于当前使用 `experimental/local-ipc-protocol` 的 mock 软件组合。YAML、`schema_id`、version 或 hash 的存在只表示结构化输入已经保存，不表示 code generation、drift gate、Server compatibility metadata 或 connect-time negotiation 已实现。

若恢复 RNG1 迁移，才需确认同源 schema 的唯一 owner、兼容策略、release manifest 影响、实际消费者和验证组合；在此之前不得把候选快照写成当前生产消费者正在使用的 schema 来源。

对 mutating command、安全关键配置、profile、calibration 和 capability 的未知字段、未知 enum、未知 unit、未知 resource、schema mismatch 或 migration 缺失，目标协议仍应 fail fast。telemetry 可以增加字段，但稳定 path、unit、quality、event id 与 reason 语义不得漂移。

`check_registry.yaml` 是原 Server profile checker 使用的 metadata 输入。该 checker 未包含在此仓；文件随接口候选快照保留，不能被误解为本仓已提供 release/profile 验证工具。

这些 schema 也不定义 systemd 服务、Maintenance 更新恢复、Ubuntu + PREEMPT_RT 线程策略或执行器总线所有权；此类产品部署边界由各自 owner 的设计和实现确定。
