//! Kimi Code 用户级 MCP 配置（~/.kimi-code/mcp.json）读写助手
//!
//! 文件格式与 Claude 兼容：`{"mcpServers": { "<id>": { ... } }}`，
//! 额外支持 url/headers/enabled/startupTimeoutMs/toolTimeoutMs/
//! enabledTools/disabledTools/bearerTokenEnvVar 等字段（原样透传）。

use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::atomic_write;
use crate::error::AppError;
use crate::kimicode_config::get_kimicode_dir;

pub fn get_kimicode_mcp_path() -> PathBuf {
    get_kimicode_dir().join("mcp.json")
}

fn read_json_value(path: &Path) -> Result<Value, AppError> {
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let content = fs::read_to_string(path).map_err(|e| AppError::io(path, e))?;
    let value: Value = serde_json::from_str(&content).map_err(|e| AppError::json(path, e))?;
    Ok(value)
}

fn write_json_value(path: &Path, value: &Value) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
    }
    let json =
        serde_json::to_string_pretty(value).map_err(|e| AppError::JsonSerialize { source: e })?;
    atomic_write(path, json.as_bytes())
}

/// 读取 mcp.json 原始文本（不存在时返回 None）
pub fn read_mcp_json() -> Result<Option<String>, AppError> {
    let path = get_kimicode_mcp_path();
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?;
    Ok(Some(content))
}

/// 读取 mcp.json 中的 mcpServers 映射
pub fn read_mcp_servers_map() -> Result<HashMap<String, Value>, AppError> {
    read_mcp_servers_map_from(&get_kimicode_mcp_path())
}

fn read_mcp_servers_map_from(path: &Path) -> Result<HashMap<String, Value>, AppError> {
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let root = read_json_value(path)?;
    let servers = root
        .get("mcpServers")
        .and_then(|v| v.as_object())
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    Ok(servers)
}

/// 将给定的启用 MCP 服务器映射写入 mcp.json 的 mcpServers 字段
/// 仅覆盖 mcpServers，其他字段保持不变
pub fn set_mcp_servers_map(servers: &HashMap<String, Value>) -> Result<(), AppError> {
    set_mcp_servers_map_to(&get_kimicode_mcp_path(), servers)
}

fn set_mcp_servers_map_to(path: &Path, servers: &HashMap<String, Value>) -> Result<(), AppError> {
    let mut root = if path.exists() {
        read_json_value(path)?
    } else {
        serde_json::json!({})
    };

    // 构建 mcpServers 对象：移除 UI 辅助字段，仅保留实际 MCP 规范
    let mut out: Map<String, Value> = Map::new();
    for (id, spec) in servers.iter() {
        let mut obj = if let Some(map) = spec.as_object() {
            map.clone()
        } else {
            return Err(AppError::McpValidation(format!(
                "MCP 服务器 '{id}' 不是对象"
            )));
        };

        if let Some(server_val) = obj.remove("server") {
            let server_obj = server_val.as_object().cloned().ok_or_else(|| {
                AppError::McpValidation(format!("MCP 服务器 '{id}' server 字段不是对象"))
            })?;
            obj = server_obj;
        }

        obj.remove("enabled");
        obj.remove("source");
        obj.remove("id");
        obj.remove("name");
        obj.remove("description");
        obj.remove("tags");
        obj.remove("homepage");
        obj.remove("docs");

        out.insert(id.clone(), Value::Object(obj));
    }

    {
        let obj = root
            .as_object_mut()
            .ok_or_else(|| AppError::Config("mcp.json 根必须是对象".into()))?;
        obj.insert("mcpServers".into(), Value::Object(out));
    }

    write_json_value(path, &root)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 每个测试用独立的临时文件路径，不碰环境变量（测试并行运行）
    fn temp_mcp_path(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ccswitch-kimicode-mcp-test-{}-{}",
            std::process::id(),
            tag
        ));
        fs::create_dir_all(&dir).unwrap();
        dir.join("mcp.json")
    }

    #[test]
    fn set_and_read_roundtrip() {
        let path = temp_mcp_path("roundtrip");
        let mut servers = HashMap::new();
        servers.insert(
            "context7".to_string(),
            json!({"command": "npx", "args": ["-y", "@upstash/context7-mcp"]}),
        );
        set_mcp_servers_map_to(&path, &servers).unwrap();

        assert!(path.exists());

        let map = read_mcp_servers_map_from(&path).unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(map["context7"]["command"], "npx");
    }

    #[test]
    fn strips_ui_fields_and_unwraps_server() {
        let path = temp_mcp_path("strip");
        let mut servers = HashMap::new();
        servers.insert(
            "web".to_string(),
            json!({
                "server": {"url": "https://example.com/mcp", "type": "http"},
                "enabled": true,
                "source": "ui",
                "id": "web",
                "name": "Web"
            }),
        );
        set_mcp_servers_map_to(&path, &servers).unwrap();

        let map = read_mcp_servers_map_from(&path).unwrap();
        let spec = &map["web"];
        assert_eq!(spec["url"], "https://example.com/mcp");
        assert!(spec.get("server").is_none());
        assert!(spec.get("enabled").is_none());
        assert!(spec.get("source").is_none());
        assert!(spec.get("name").is_none());
    }

    #[test]
    fn preserves_other_root_keys() {
        let path = temp_mcp_path("preserve");
        fs::write(&path, json!({"other": 1, "mcpServers": {}}).to_string()).unwrap();

        let mut servers = HashMap::new();
        servers.insert("a".to_string(), json!({"command": "x"}));
        set_mcp_servers_map_to(&path, &servers).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let v: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(v["other"], 1);
        assert!(v["mcpServers"]["a"].is_object());
    }

    #[test]
    fn read_missing_file_returns_empty() {
        let path = temp_mcp_path("missing");
        let map = read_mcp_servers_map_from(&path).unwrap();
        assert!(map.is_empty());
    }
}
