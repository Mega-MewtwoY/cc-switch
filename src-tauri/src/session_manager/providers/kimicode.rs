//! Kimi Code 会话管理（~/.kimi-code/sessions/<workspace>/<session>/）
//!
//! 布局（已核实）：
//! - state.json：id/cwd/createdAt/updatedAt/title/lastPrompt/archived（时间均为 epoch ms）
//! - agents/<agent>/wire.jsonl：事件流。用户消息为顶层 `turn.prompt`；
//!   助手文本/工具调用嵌套在 `context.append_loop_event.event` 内
//!   （content.part part.type=="text" / tool.call）。

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::session_manager::{SessionMessage, SessionMeta};

use super::utils::truncate_summary;

const PROVIDER_ID: &str = "kimicode";

pub fn session_root() -> PathBuf {
    crate::kimicode_config::get_kimicode_dir().join("sessions")
}

pub fn scan_sessions() -> Vec<SessionMeta> {
    let root = session_root();
    if !root.exists() {
        return Vec::new();
    }

    let mut sessions = Vec::new();

    // 固定深度：sessions/<workspace>/<session>/state.json
    let workspaces = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    for ws_entry in workspaces.flatten() {
        let ws_path = ws_entry.path();
        if !ws_path.is_dir() {
            continue;
        }
        let session_dirs = match std::fs::read_dir(&ws_path) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for sess_entry in session_dirs.flatten() {
            let sess_path = sess_entry.path();
            if !sess_path.is_dir() {
                continue;
            }
            if let Some(meta) = parse_session(&sess_path) {
                sessions.push(meta);
            }
        }
    }

    sessions
}

fn parse_session(session_dir: &Path) -> Option<SessionMeta> {
    let state_path = session_dir.join("state.json");
    let data = std::fs::read_to_string(&state_path).ok()?;
    let value: Value = serde_json::from_str(&data).ok()?;

    let session_id = value.get("id").and_then(Value::as_str)?.to_string();

    // 跳过已归档会话（kimi 自身的归档语义=不再展示）
    if value.get("archived").and_then(Value::as_bool) == Some(true) {
        return None;
    }

    let created_at = value.get("createdAt").and_then(Value::as_i64);
    let last_active_at = value.get("updatedAt").and_then(Value::as_i64);

    let title = value
        .get("title")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(|s| truncate_summary(s, 160))
        .or_else(|| {
            value
                .get("lastPrompt")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(|s| truncate_summary(s, 160))
        });

    let project_dir = value
        .get("cwd")
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    Some(SessionMeta {
        provider_id: PROVIDER_ID.to_string(),
        session_id: session_id.clone(),
        title: title.clone(),
        summary: title,
        project_dir,
        created_at,
        last_active_at: last_active_at.or(created_at),
        source_path: Some(session_dir.to_string_lossy().to_string()),
        resume_command: Some(format!("kimi -S {session_id}")),
    })
}

/// source_path 是会话目录；读取其中所有 agent 的 wire.jsonl
pub fn load_messages(source_dir: &Path) -> Result<Vec<SessionMessage>, String> {
    let agents_dir = source_dir.join("agents");
    let entries = std::fs::read_dir(&agents_dir)
        .map_err(|e| format!("Failed to read agents dir: {e}"))?;

    let mut result = Vec::new();
    for entry in entries.flatten() {
        let wire_path = entry.path().join("wire.jsonl");
        if !wire_path.is_file() {
            continue;
        }
        let data = std::fs::read_to_string(&wire_path)
            .map_err(|e| format!("Failed to read wire.jsonl: {e}"))?;
        for line in data.lines() {
            if let Some(msg) = parse_wire_line(line) {
                result.push(msg);
            }
        }
    }

    // 多 agent 文件按读取顺序拼接后，按时间戳重排
    result.sort_by_key(|m| m.ts.unwrap_or(0));
    Ok(result)
}

/// 解析一行 wire.jsonl 为消息；非消息事件返回 None
fn parse_wire_line(line: &str) -> Option<SessionMessage> {
    let value: Value = serde_json::from_str(line).ok()?;

    match value.get("type").and_then(Value::as_str) {
        // 用户输入（顶层事件）
        Some("turn.prompt") => {
            if value
                .get("origin")
                .and_then(|o| o.get("kind"))
                .and_then(Value::as_str)
                != Some("user")
            {
                return None;
            }
            let content = value
                .get("input")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter(|i| i.get("type").and_then(Value::as_str) == Some("text"))
                        .filter_map(|i| i.get("text").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default();
            if content.trim().is_empty() {
                return None;
            }
            Some(SessionMessage {
                role: "user".to_string(),
                content,
                ts: value.get("time").and_then(Value::as_i64),
            })
        }
        // 助手事件（嵌套在 loop_event 内）
        Some("context.append_loop_event") => {
            let event = value.get("event")?;
            let ts = value.get("time").and_then(Value::as_i64);
            match event.get("type").and_then(Value::as_str) {
                Some("content.part") => {
                    let part = event.get("part")?;
                    if part.get("type").and_then(Value::as_str) != Some("text") {
                        return None;
                    }
                    let content = part.get("text").and_then(Value::as_str)?.to_string();
                    if content.trim().is_empty() {
                        return None;
                    }
                    Some(SessionMessage {
                        role: "assistant".to_string(),
                        content,
                        ts,
                    })
                }
                Some("tool.call") => {
                    let name = event.get("name").and_then(Value::as_str)?;
                    Some(SessionMessage {
                        role: "assistant".to_string(),
                        content: format!("[Tool: {name}]"),
                        ts,
                    })
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// 删除整个会话目录（source 即会话目录，已在 mod.rs 校验位于 root 内）
pub fn delete_session(root: &Path, source: &Path, session_id: &str) -> Result<bool, String> {
    // 防误删：目录名必须与 session_id 一致，且确实是 root 的直接孙级
    let dir_name = source
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if dir_name != session_id {
        return Err(format!(
            "Kimi Code session ID mismatch: expected {session_id}, dir {dir_name}"
        ));
    }
    if source.parent().and_then(Path::parent) != Some(root) {
        return Err(format!(
            "Kimi Code session path is not a direct session dir: {}",
            source.display()
        ));
    }

    std::fs::remove_dir_all(source).map_err(|e| {
        format!(
            "Failed to delete Kimi Code session dir {}: {e}",
            source.display()
        )
    })?;

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_turn_prompt_as_user_message() {
        let line = r#"{"type":"turn.prompt","input":[{"type":"text","text":"如何配置状态栏？"}],"origin":{"kind":"user"},"time":1786363396213}"#;
        let msg = parse_wire_line(line).unwrap();
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, "如何配置状态栏？");
        assert_eq!(msg.ts, Some(1786363396213));
    }

    #[test]
    fn parses_nested_text_part_as_assistant() {
        let line = r#"{"type":"context.append_loop_event","event":{"type":"content.part","part":{"type":"text","text":"我先查看文档。"}},"time":1786363403331}"#;
        let msg = parse_wire_line(line).unwrap();
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, "我先查看文档。");
    }

    #[test]
    fn parses_tool_call_as_marker() {
        let line = r#"{"type":"context.append_loop_event","event":{"type":"tool.call","name":"Read","args":{"path":"README.md"}},"time":1786363403364}"#;
        let msg = parse_wire_line(line).unwrap();
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, "[Tool: Read]");
    }

    #[test]
    fn skips_non_message_events() {
        assert!(parse_wire_line(r#"{"type":"metadata","protocol_version":"1.5"}"#).is_none());
        assert!(
            parse_wire_line(r#"{"type":"usage.record","usageScope":"turn"}"#).is_none()
        );
        // think 片段不展示
        assert!(parse_wire_line(
            r#"{"type":"context.append_loop_event","event":{"type":"content.part","part":{"type":"think","think":"…"}},"time":1}"#
        )
        .is_none());
        // 非 user 来源的 prompt 不展示
        assert!(parse_wire_line(
            r#"{"type":"turn.prompt","input":[{"type":"text","text":"x"}],"origin":{"kind":"system"},"time":1}"#
        )
        .is_none());
        assert!(parse_wire_line("not json").is_none());
    }

    #[test]
    fn parses_state_json_session_meta() {
        let temp = std::env::temp_dir().join(format!(
            "ccswitch-kimicode-session-test-{}",
            std::process::id()
        ));
        let session_dir = temp.join("wd_proj_abc/session_1");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(
            session_dir.join("state.json"),
            r#"{"id":"session_1","version":2,"cwd":"/work/proj","createdAt":1786363006990,"updatedAt":1786367931620,"archived":false,"title":"如何配置状态栏？","lastPrompt":"你好","isCustomTitle":false}"#,
        )
        .unwrap();

        let meta = parse_session(&session_dir).unwrap();
        assert_eq!(meta.provider_id, "kimicode");
        assert_eq!(meta.session_id, "session_1");
        assert_eq!(meta.title.as_deref(), Some("如何配置状态栏？"));
        assert_eq!(meta.project_dir.as_deref(), Some("/work/proj"));
        assert_eq!(meta.created_at, Some(1786363006990));
        assert_eq!(meta.last_active_at, Some(1786367931620));
        assert_eq!(meta.resume_command.as_deref(), Some("kimi -S session_1"));

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn skips_archived_sessions() {
        let temp = std::env::temp_dir().join(format!(
            "ccswitch-kimicode-archived-test-{}",
            std::process::id()
        ));
        let session_dir = temp.join("wd_proj_abc/session_2");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(
            session_dir.join("state.json"),
            r#"{"id":"session_2","archived":true,"createdAt":1,"updatedAt":2}"#,
        )
        .unwrap();

        assert!(parse_session(&session_dir).is_none());

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn delete_session_removes_session_dir() {
        let temp = std::env::temp_dir().join(format!(
            "ccswitch-kimicode-delete-test-{}",
            std::process::id()
        ));
        let root = temp.join("sessions");
        let session_dir = root.join("wd_proj_abc/session_3");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(session_dir.join("state.json"), "{}").unwrap();

        let canonical_root = root.canonicalize().unwrap();
        let canonical_source = session_dir.canonicalize().unwrap();
        delete_session(&canonical_root, &canonical_source, "session_3").unwrap();
        assert!(!session_dir.exists());

        let _ = std::fs::remove_dir_all(&temp);
    }
}
