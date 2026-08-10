//! Kimi Code MCP 同步模块（~/.kimi-code/mcp.json）

use serde_json::Value;

use crate::app_config::{McpApps, McpServer, MultiAppConfig};
use crate::error::AppError;

use super::validation::validate_server_spec;

fn should_sync_kimicode_mcp() -> bool {
    // Kimi Code 未安装/未初始化时跳过写入，不创建任何文件或目录
    crate::kimicode_config::get_kimicode_dir().exists()
        || crate::kimicode_mcp::get_kimicode_mcp_path().exists()
}

/// 从 ~/.kimi-code/mcp.json 导入 mcpServers 到统一结构
/// 已存在的服务器将启用 KimiCode 应用，不覆盖其他字段和应用状态
pub fn import_from_kimicode(config: &mut MultiAppConfig) -> Result<usize, AppError> {
    let text_opt = crate::kimicode_mcp::read_mcp_json()?;
    let Some(text) = text_opt else { return Ok(0) };

    let v: Value = serde_json::from_str(&text)
        .map_err(|e| AppError::McpValidation(format!("解析 ~/.kimi-code/mcp.json 失败: {e}")))?;
    let Some(map) = v.get("mcpServers").and_then(|x| x.as_object()) else {
        return Ok(0);
    };

    let servers = config.mcp.servers.get_or_insert_with(std::collections::HashMap::new);

    let mut changed = 0;
    let mut errors = Vec::new();

    for (id, spec) in map.iter() {
        if let Err(e) = validate_server_spec(spec) {
            log::warn!("跳过无效 MCP 服务器 '{id}': {e}");
            errors.push(format!("{id}: {e}"));
            continue;
        }

        if let Some(existing) = servers.get_mut(id) {
            // 已存在：仅启用 KimiCode 应用
            if !existing.apps.kimicode {
                existing.apps.kimicode = true;
                changed += 1;
                log::info!("MCP 服务器 '{id}' 已启用 KimiCode 应用");
            }
        } else {
            // 新建服务器：默认仅启用 KimiCode
            servers.insert(
                id.clone(),
                McpServer {
                    id: id.clone(),
                    name: id.clone(),
                    server: spec.clone(),
                    apps: McpApps {
                        kimicode: true,
                        ..Default::default()
                    },
                    description: None,
                    homepage: None,
                    docs: None,
                    tags: Vec::new(),
                },
            );
            changed += 1;
            log::info!("从 Kimi Code 导入新 MCP 服务器 '{id}'");
        }
    }

    if !errors.is_empty() {
        log::warn!("导入完成，但有 {} 项失败: {:?}", errors.len(), errors);
    }

    Ok(changed)
}

/// 将单个 MCP 服务器同步到 Kimi Code live 配置
pub fn sync_single_server_to_kimicode(
    _config: &MultiAppConfig,
    id: &str,
    server_spec: &Value,
) -> Result<(), AppError> {
    if !should_sync_kimicode_mcp() {
        return Ok(());
    }
    let current = crate::kimicode_mcp::read_mcp_servers_map()?;

    let mut updated = current;
    updated.insert(id.to_string(), server_spec.clone());

    crate::kimicode_mcp::set_mcp_servers_map(&updated)
}

/// 从 Kimi Code live 配置中移除单个 MCP 服务器
pub fn remove_server_from_kimicode(id: &str) -> Result<(), AppError> {
    if !should_sync_kimicode_mcp() {
        return Ok(());
    }
    let mut current = crate::kimicode_mcp::read_mcp_servers_map()?;

    current.remove(id);

    crate::kimicode_mcp::set_mcp_servers_map(&current)
}
