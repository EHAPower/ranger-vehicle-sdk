# SDK Examples

本目录保存 Ranger Client SDK 的目标示例。示例应面向 SDK 使用者，而不是 Server 内部开发者。

## 目标示例

| 示例 | 用途 | 默认安全边界 |
|------|------|--------------|
| `ranger_cli/` | operator / maintenance CLI 如何通过 SDK 调用 Ranger Server | 默认只读或 dry-run；动作命令必须显式标注 |
| `dimos_client/` | dimos 如何提交车辆级 command、heartbeat 和读取 receipt | 不直接写 wheel / EHA；必须经过 Server command gate |
| `telemetry_watch/` | 订阅 diagnostics / status / readiness | 只读，不反压 Server |

## 编写规则

- 示例只调用 SDK API，不导入 `crates/daemon` 或 `crates/hal` 内部模块。
- 示例必须区分 request accepted、command admitted、backend committed、observed / completed。
- 示例必须展示错误处理和超时处理，不只展示 happy path。
- 真实硬件动作示例默认不落地到这里；如确需提供，必须先有安全 SOP 和明确的 Server-side guard。
