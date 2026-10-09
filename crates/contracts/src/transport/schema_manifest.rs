//! Ranger Server API 的严格 schema 身份和连接协商。
//!
//! 这里仅登记 Server API 实际收发的三份 schema。它们的版本和语义 hash
//! 来自 `sdk/schema` 的正式 schema 集合；任一项缺失、重复或不完全相同，
//! 控制会话都不能建立。

use super::bootstrap::{
    ClientSchemaSupport, CompatibilityResultRepr, ConnectStatusRepr, SchemaIdRepr,
    SchemaNegotiation, SchemaVersionHash,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiSchemaIdentity {
    pub schema_id: SchemaIdRepr,
    pub version: &'static str,
    pub hash: &'static str,
}

/// WP_VER=2 的 Server API 控制平面必须精确匹配的 schema。
pub const STRICT_API_SCHEMA_IDENTITIES: [ApiSchemaIdentity; 3] = [
    ApiSchemaIdentity {
        schema_id: SchemaIdRepr::CommandRequest,
        version: "ranger.command_request.v2",
        hash: "sha256:0c90d3256e6d6b94842a9adbb61e795e880d78d44dd3f7721b072ebe89be258a",
    },
    ApiSchemaIdentity {
        schema_id: SchemaIdRepr::ReceiptEvent,
        version: "ranger.receipt_event.v2",
        hash: "sha256:75bdc812f123e41fe478bef401990d6ca7c3a4815265d2b8212693a271fecf0b",
    },
    ApiSchemaIdentity {
        schema_id: SchemaIdRepr::TelemetryDiagnostics,
        version: "ranger.telemetry_diagnostics.v2",
        hash: "sha256:758cb9b9823a83cfe1046cee80c82b8c257d9afda90b90ce5486adea4514344b",
    },
];

/// 构造 ConnectRequest 所需的唯一、精确 v2 API schema manifest。
pub fn strict_api_schema_manifest() -> Vec<ClientSchemaSupport> {
    STRICT_API_SCHEMA_IDENTITIES
        .iter()
        .map(|identity| ClientSchemaSupport {
            schema_id: identity.schema_id,
            supported_versions: vec![SchemaVersionHash {
                version: identity.version.to_owned(),
                hash: identity.hash.to_owned(),
            }],
        })
        .collect()
}

/// 对客户端 manifest 执行 v2 控制面的精确协商。
///
/// 每个必需 schema 都必须恰有一个 version/hash 都相同的条目。返回的结果
/// 总会逐项说明 server 的正式身份，使客户端能够同样校验服务器响应。
pub fn negotiate_strict_api_schemas(
    client_schemas: &[ClientSchemaSupport],
) -> (ConnectStatusRepr, Vec<SchemaNegotiation>) {
    let negotiations = STRICT_API_SCHEMA_IDENTITIES
        .iter()
        .map(|identity| {
            let matching_entries = client_schemas
                .iter()
                .filter(|entry| entry.schema_id == identity.schema_id)
                .collect::<Vec<_>>();
            let compatible = matching_entries.len() == 1
                && matching_entries[0].supported_versions.len() == 1
                && matching_entries[0].supported_versions[0].version == identity.version
                && matching_entries[0].supported_versions[0].hash == identity.hash;
            let compatibility = if compatible {
                CompatibilityResultRepr::Compatible
            } else if matching_entries.iter().any(|entry| {
                entry
                    .supported_versions
                    .iter()
                    .any(|version| version.version == identity.version)
            }) {
                CompatibilityResultRepr::HashDrift
            } else {
                CompatibilityResultRepr::NoCommonVersion
            };
            SchemaNegotiation {
                schema_id: identity.schema_id,
                selected_version: compatible.then(|| identity.version.to_owned()),
                selected_hash: compatible.then(|| identity.hash.to_owned()),
                compatibility,
            }
        })
        .collect::<Vec<_>>();
    let status = if negotiations
        .iter()
        .all(|item| item.compatibility == CompatibilityResultRepr::Compatible)
    {
        ConnectStatusRepr::Ok
    } else {
        ConnectStatusRepr::SchemaIncompatible
    };
    (status, negotiations)
}

/// 确认服务器宣告的协商结果仍等于本 SDK 编译进的严格 manifest。
pub fn validate_strict_api_schema_response(response: &[SchemaNegotiation]) -> Result<(), String> {
    if response.len() != STRICT_API_SCHEMA_IDENTITIES.len() {
        return Err(format!(
            "Server schema negotiation 数量不匹配：{} != {}",
            response.len(),
            STRICT_API_SCHEMA_IDENTITIES.len()
        ));
    }
    for identity in STRICT_API_SCHEMA_IDENTITIES {
        let matches = response
            .iter()
            .filter(|item| item.schema_id == identity.schema_id)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(format!(
                "Server schema negotiation 缺失或重复：{:?}",
                identity.schema_id
            ));
        }
        let item = matches[0];
        if item.compatibility != CompatibilityResultRepr::Compatible
            || item.selected_version.as_deref() != Some(identity.version)
            || item.selected_hash.as_deref() != Some(identity.hash)
        {
            return Err(format!(
                "Server schema negotiation 不兼容：{:?}",
                identity.schema_id
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_manifest_is_accepted_and_roundtrips_server_identity() {
        let manifest = strict_api_schema_manifest();
        let (status, response) = negotiate_strict_api_schemas(&manifest);

        assert_eq!(status, ConnectStatusRepr::Ok);
        validate_strict_api_schema_response(&response).expect("exact response accepted");
    }

    #[test]
    fn missing_or_drifted_schema_rejects_control_connection() {
        let mut missing = strict_api_schema_manifest();
        missing.pop();
        assert_eq!(
            negotiate_strict_api_schemas(&missing).0,
            ConnectStatusRepr::SchemaIncompatible
        );

        let mut drifted = strict_api_schema_manifest();
        drifted[0].supported_versions[0].hash = "sha256:drift".to_owned();
        let (status, response) = negotiate_strict_api_schemas(&drifted);
        assert_eq!(status, ConnectStatusRepr::SchemaIncompatible);
        assert_eq!(
            response[0].compatibility,
            CompatibilityResultRepr::HashDrift
        );
    }
}
