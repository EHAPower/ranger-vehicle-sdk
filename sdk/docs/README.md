# SDK Docs

本目录保存 Ranger Client SDK 的 API、兼容性和发布说明。

计划文档：

| 文档 | 用途 |
|------|------|
| `api.md` | Server API、request / response、receipt、diagnostics subscription |
| `compatibility.md` | SDK version、Server API schema version、release compatibility |
| `schema.md` | command / event / telemetry schema 说明和生成 / 校验规则 |
| `errors.md` | 错误码、超时、拒绝原因、执行状态解释 |
| `auth.md` | client identity、权限、审计和安全边界 |

这些文件在对应 open question 或 spec 关闭前不应伪造稳定合同。当前正式集合和 hash 规则只在 [schema 说明](../schema/README.md) 维护；车辆接口与 profile 语义分别归 [docs/specs/ranger-vehicle-interface.md](https://github.com/EHAPower/ranger-vehicle-runtime/blob/main/docs/specs/ranger-vehicle-interface.md) 和 [docs/specs/ranger-platform-profile.md](https://github.com/EHAPower/ranger-vehicle-runtime/blob/main/docs/specs/ranger-platform-profile.md)。没有运行消费者的 parameter schema 已退出正式集合，通用参数写入仍未实现。三项 Server API schema 已实现 exact version/hash negotiation；DTO codegen 尚未实现。Runtime 的 checker 和真实 Server 集成测试消费本仓固定 SDK revision，不属于 SDK 的反向依赖。
