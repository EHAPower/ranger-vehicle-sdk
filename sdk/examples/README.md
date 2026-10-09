# SDK 示例

本目录保存 Ranger Client SDK 的示例约束。当前提取快照尚未包含可独立运行的示例程序；不要把 Server 内部命令、硬件路径或部署脚本复制进来作为 SDK 示例。

后续示例应只调用 SDK API，并展示错误和超时处理。示例必须区分 request accepted、backend committed、执行完成和 stop confirmation。真实硬件动作示例需要独立的安全 SOP 与 Server-side guard。
