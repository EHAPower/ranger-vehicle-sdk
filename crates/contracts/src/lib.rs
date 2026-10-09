//! Ranger Vehicle SDK / Runtime 共享 DTO、transport envelope 和 schema 协商类型。
//!
//! 本 crate 是纯数据层，不依赖 `daemon`、`hal`、`control` 或具体 backend。
//! DTO 来源：
//! - `transport::bootstrap`：硬编码的 ConnectRequest/Response，固定在 wire protocol version 内。
//! - `generated`：手写 DTO，与本仓 `sdk/schema/*.yaml` 对齐；自动 codegen 尚未实现。

pub mod generated;
pub mod transport;
