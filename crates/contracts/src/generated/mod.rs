//! v1 手写 DTO，对应 `sdk/schema/*.yaml` 定义。
//!
//! 等 codegen spike 完成后，本模块内容由生成器产出。
//! 当前手写版本必须与 YAML schema 语义一致。

pub mod command;
pub mod receipt;
pub mod session;
pub mod telemetry;
