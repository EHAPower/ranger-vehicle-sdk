//! Ranger Vehicle SDK 维护的正式 schema 和 checker registry 原始源文本。
//!
//! 本包只提供结构化输入，不解析 YAML、不计算身份，也不包含 checker 算法。
//! Runtime 的 `profile_check` 在构建脚本和测试中消费相同文本。

/// 八份正式 schema 的稳定文件名与原始 YAML 文本。
pub const FORMAL_SCHEMA_SOURCES: [(&str, &str); 8] = [
    ("active_config.yaml", include_str!("../active_config.yaml")),
    (
        "command_request.yaml",
        include_str!("../command_request.yaml"),
    ),
    ("field_bundle.yaml", include_str!("../field_bundle.yaml")),
    (
        "platform_profile.yaml",
        include_str!("../platform_profile.yaml"),
    ),
    ("receipt_event.yaml", include_str!("../receipt_event.yaml")),
    (
        "release_manifest.yaml",
        include_str!("../release_manifest.yaml"),
    ),
    (
        "runtime_capability.yaml",
        include_str!("../runtime_capability.yaml"),
    ),
    (
        "telemetry_diagnostics.yaml",
        include_str!("../telemetry_diagnostics.yaml"),
    ),
];

/// Checker registry metadata 原始文本，不计入八份 schema aggregate。
pub const CHECK_REGISTRY_SOURCE: &str = include_str!("../check_registry.yaml");
