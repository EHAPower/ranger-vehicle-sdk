//! Ranger Vehicle SDK / Runtime 共享 DTO、transport envelope 和 schema 协商类型。
//!
//! 本 crate 是纯数据层，不依赖 `daemon`、`hal`、`control` 或具体 backend。
//! DTO 来源：
//! - `transport::bootstrap`：硬编码的 ConnectRequest/Response，固定在 wire protocol version 内。
//! - `generated`：从 `sdk/schema/*.yaml` 生成，首版由手写 DTO 占位，等 codegen spike 完成后切换。

pub mod generated;
pub mod transport;
