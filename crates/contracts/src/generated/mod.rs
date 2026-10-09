//! 当前 active schema 的手写 DTO，对应 `sdk/schema/*.yaml` 定义。
//!
//! 等 codegen spike 完成后，本模块内容由生成器产出。
//! active 模块必须与 YAML schema 语义一致；v2 lifecycle 已分配 WP_VER=2
//! wire message，但 DTO 存在不能当作当前 runtime capability。

pub mod command;
pub mod lifecycle;
pub mod receipt;
pub mod session;
pub mod telemetry;
