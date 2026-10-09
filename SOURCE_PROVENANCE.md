# 源码来源与维护归属

`ranger-vehicle-sdk` 是 `crates/contracts`、`sdk/rust`、`sdk/schema` 及对应 SDK 文档、示例与 fixture 的唯一源码维护 owner。Runtime / Apps 通过仓内 SDK submodule 和 Cargo path 消费固定版本，集成仓用 gitlink 固定一致的组件组合。

## 提取与迁移记录

- 首次接口快照来自 `ranger-vehicle-runtime`（原 `ranger-server`）的 [`a691ce1a999314eae5634876786de182915509cb`](https://github.com/EHAPower/ranger-vehicle-runtime/commit/a691ce1a999314eae5634876786de182915509cb)，用于独立仓初始提取；它不代表现行 wire / schema 基线。
- 2026-10-09 当前完整迁移来自 Runtime [`11780dca1518fd9002e2bf8ff7e5179986234027`](https://github.com/EHAPower/ranger-vehicle-runtime/commit/11780dca1518fd9002e2bf8ff7e5179986234027) 的 `crates/contracts` 和 `sdk`，导入 wire v2、当前 Rust Client、八份正式 schema、registry、文档、示例说明及源内测试 fixture。没有引入旧 `parameter_config.yaml`。
- Runtime 同路径的公共源码随消费者切换移除，改为引用 `deps/ranger-vehicle-sdk`；运行进程、硬件生命周期和安全裁决继续由 Runtime 持有。

## 验证与算法边界

SDK 保留 contracts codec / DTO / negotiation 测试和 Client socket mock 测试。原 `sdk/rust/tests/server_api_e2e.rs` 的完整真实 Server 集成测试，以及原 contracts 的 `strict_identities_match_the_formal_schema_entries` 验证，迁入 Runtime。SDK 不反向依赖 `server_api`、`vehicle_core`、`msgs` 或 `profile_check`。

`ranger-schema-sources` 只打包八份正式 YAML 与 `check_registry.yaml` 的原始文本。解析、semantic hash、registry 生成、checker decision 与 `ranger-check` 算法留在 Runtime `crates/profile_check`；它们不是 SDK 源包的复制品。

## 作者、版权与许可

以上路径由 EHAPower Ranger 项目维护，原作者与修改历史以来源仓和本仓 Git history 为准。仓库位置与维护 owner 的迁移不改变既有作者或版权归属；已有第三方 copyright / license / 来源说明应原样保留。

该导入基线的公共源码路径及独立 SDK 仓没有仓级 `LICENSE`、`COPYRIGHT`、`NOTICE` 或独立 SDK 许可声明。本记录只说明来源与维护归属，不新增、推定或变更许可授权；package metadata 也不虚构 license。
