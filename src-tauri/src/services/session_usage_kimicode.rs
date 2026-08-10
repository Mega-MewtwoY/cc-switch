//! Kimi Code 会话日志使用追踪
//!
//! 从 ~/.kimi-code/sessions/<workspace>/<session>/agents/<agent>/wire.jsonl
//! 中提取 usage.record 事件的 token 使用数据，实现无代理模式下的使用统计。
//!
//! ## 数据流
//! ```text
//! wire.jsonl usage.record (usageScope=="turn")
//!   → 增量解析（行偏移） → 去重 → 费用计算 → proxy_request_logs 表
//! ```
//!
//! 事件格式（已核实）：
//! `{"type":"usage.record","model":"kimi-code/k3","usage":{"inputOther":…,
//! "output":…,"inputCacheRead":…,"inputCacheCreation":…},"usageScope":"turn",
//! "time":<epoch_ms>}`

use crate::database::{lock_conn, Database};
use crate::error::AppError;
use crate::kimicode_config::get_kimicode_dir;
use crate::proxy::usage::calculator::CostCalculator;
use crate::proxy::usage::parser::TokenUsage;
use crate::services::session_usage::{
    get_sync_state, metadata_modified_nanos, update_sync_state, SessionSyncResult,
};
use crate::services::usage_stats::{find_model_pricing, should_skip_session_insert, DedupKey};
use rust_decimal::Decimal;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// 一个待同步的 wire.jsonl 文件及其归属
struct KimiWireFile {
    path: PathBuf,
    session_id: String,
    agent: String,
}

/// 从 usage.record 事件中解析出的使用数据
struct KimiUsageRecord {
    model: String,
    input_tokens: u32,
    output_tokens: u32,
    cache_read_tokens: u32,
    cache_creation_tokens: u32,
    timestamp_ms: i64,
}

/// 同步 Kimi Code 使用数据
pub fn sync_kimicode_usage(db: &Database) -> Result<SessionSyncResult, AppError> {
    let sessions_root = get_kimicode_dir().join("sessions");
    if !sessions_root.exists() {
        return Ok(SessionSyncResult::default());
    }

    let mut result = SessionSyncResult::default();
    let wire_files = collect_wire_files(&sessions_root);

    for file in &wire_files {
        result.files_scanned += 1;
        match sync_single_wire_file(db, file) {
            Ok((imported, skipped)) => {
                result.imported += imported;
                result.skipped += skipped;
            }
            Err(e) => {
                let msg = format!("{}: {e}", file.path.display());
                log::warn!("[KIMICODE-SYNC] 文件解析失败: {msg}");
                result.errors.push(msg);
            }
        }
    }

    if result.imported > 0 {
        log::info!(
            "[KIMICODE-SYNC] 同步完成: 导入 {} 条, 跳过 {} 条, 扫描 {} 个文件",
            result.imported,
            result.skipped,
            result.files_scanned
        );
    }

    Ok(result)
}

/// 收集所有 wire.jsonl 文件（固定深度，不递归）：
/// sessions/<workspace>/<session>/agents/<agent>/wire.jsonl
fn collect_wire_files(sessions_root: &Path) -> Vec<KimiWireFile> {
    let mut files = Vec::new();

    let workspaces = match fs::read_dir(sessions_root) {
        Ok(e) => e,
        Err(_) => return files,
    };

    for ws_entry in workspaces.flatten() {
        let ws_path = ws_entry.path();
        if !ws_path.is_dir() {
            continue;
        }
        let sessions = match fs::read_dir(&ws_path) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for sess_entry in sessions.flatten() {
            let sess_path = sess_entry.path();
            if !sess_path.is_dir() {
                continue;
            }
            let session_id = sess_entry.file_name().to_string_lossy().to_string();
            let agents_dir = sess_path.join("agents");
            let agents = match fs::read_dir(&agents_dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for agent_entry in agents.flatten() {
                let agent_path = agent_entry.path();
                if !agent_path.is_dir() {
                    continue;
                }
                let wire_path = agent_path.join("wire.jsonl");
                if wire_path.is_file() {
                    files.push(KimiWireFile {
                        path: wire_path,
                        session_id: session_id.clone(),
                        agent: agent_entry.file_name().to_string_lossy().to_string(),
                    });
                }
            }
        }
    }

    files
}

/// 同步单个 wire.jsonl 文件，返回 (imported, skipped)
fn sync_single_wire_file(db: &Database, file: &KimiWireFile) -> Result<(u32, u32), AppError> {
    let file_path_str = file.path.to_string_lossy().to_string();

    let metadata = fs::metadata(&file.path)
        .map_err(|e| AppError::Config(format!("无法读取文件元数据: {e}")))?;
    let file_modified = metadata_modified_nanos(&metadata);

    let (last_modified, last_offset) = get_sync_state(db, &file_path_str)?;

    // 文件未变化则跳过
    if file_modified <= last_modified {
        return Ok((0, 0));
    }

    let fh = fs::File::open(&file.path)
        .map_err(|e| AppError::Config(format!("无法打开文件: {e}")))?;
    let reader = BufReader::new(fh);

    let mut imported = 0u32;
    let mut skipped = 0u32;
    let mut line_offset: i64 = 0;
    let mut had_error = false;

    for line_result in reader.lines() {
        line_offset += 1;

        if line_offset <= last_offset {
            continue;
        }

        let line = match line_result {
            Ok(l) => l,
            Err(_) => continue, // 容忍不完整的最后一行
        };
        if line.trim().is_empty() {
            continue;
        }

        let Some(record) = parse_usage_record(&line) else {
            continue;
        };

        let request_id = format!(
            "kimicode_session:{}:{}:{line_offset}",
            file.session_id, file.agent
        );

        match insert_kimicode_record(db, &request_id, &record, &file.session_id) {
            Ok(true) => imported += 1,
            Ok(false) => skipped += 1,
            Err(e) => {
                log::warn!("[KIMICODE-SYNC] 记录插入失败 {request_id}: {e}");
                skipped += 1;
                had_error = true;
            }
        }
    }

    // 有插入错误时不推进游标，下次全量重扫（INSERT OR IGNORE 保证不重双）
    if !had_error {
        update_sync_state(db, &file_path_str, file_modified, line_offset)?;
    }

    Ok((imported, skipped))
}

/// 解析一行 wire.jsonl，非 usage.record(turn) 或全零 token 返回 None
fn parse_usage_record(line: &str) -> Option<KimiUsageRecord> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;

    if value.get("type").and_then(|v| v.as_str()) != Some("usage.record") {
        return None;
    }
    // 只统计 turn 级记录，避免未来出现聚合 scope 时重复计数
    if value.get("usageScope").and_then(|v| v.as_str()) != Some("turn") {
        return None;
    }

    let usage = value.get("usage")?;
    let input_tokens = usage.get("inputOther").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let output_tokens = usage.get("output").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let cache_read_tokens = usage
        .get("inputCacheRead")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let cache_creation_tokens = usage
        .get("inputCacheCreation")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    if input_tokens == 0
        && output_tokens == 0
        && cache_read_tokens == 0
        && cache_creation_tokens == 0
    {
        return None;
    }

    Some(KimiUsageRecord {
        model: value
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_creation_tokens,
        timestamp_ms: value.get("time").and_then(|v| v.as_i64()).unwrap_or(0),
    })
}

/// 插入单条 usage.record 到 proxy_request_logs，返回是否新插入
fn insert_kimicode_record(
    db: &Database,
    request_id: &str,
    record: &KimiUsageRecord,
    session_id: &str,
) -> Result<bool, AppError> {
    let conn = lock_conn!(db.conn);

    let created_at = if record.timestamp_ms > 0 {
        record.timestamp_ms / 1000
    } else {
        std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    };

    let dedup_key = DedupKey {
        app_type: "kimicode",
        model: &record.model,
        input_tokens: record.input_tokens,
        output_tokens: record.output_tokens,
        cache_read_tokens: record.cache_read_tokens,
        cache_creation_tokens: record.cache_creation_tokens,
        created_at,
    };
    if should_skip_session_insert(&conn, request_id, &dedup_key)? {
        return Ok(false);
    }

    // kimi 的 usage.record 不含费用，按模型定价表计算；无定价则为 0
    let (input_cost, output_cost, cache_read_cost, cache_creation_cost, total_cost) = {
        let usage = TokenUsage {
            input_tokens: record.input_tokens,
            output_tokens: record.output_tokens,
            cache_read_tokens: record.cache_read_tokens,
            cache_creation_tokens: record.cache_creation_tokens,
            model: Some(record.model.clone()),
            message_id: None,
        };

        match find_model_pricing(&conn, &record.model) {
            Some(pricing) => {
                let cost = CostCalculator::calculate_for_app(
                    "kimicode",
                    &usage,
                    &pricing,
                    Decimal::from(1),
                );
                (
                    cost.input_cost.to_string(),
                    cost.output_cost.to_string(),
                    cost.cache_read_cost.to_string(),
                    cost.cache_creation_cost.to_string(),
                    cost.total_cost.to_string(),
                )
            }
            None => (
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
            ),
        }
    };

    let inserted_rows = conn.execute(
        "INSERT OR IGNORE INTO proxy_request_logs (
            request_id, provider_id, app_type, model, request_model,
            input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
            input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd, total_cost_usd,
            latency_ms, first_token_ms, status_code, error_message, session_id,
            provider_type, is_streaming, cost_multiplier, created_at, data_source
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
        rusqlite::params![
            request_id,
            "_kimicode_session",   // provider_id
            "kimicode",            // app_type
            record.model,
            record.model,          // request_model = model
            record.input_tokens,
            record.output_tokens,
            record.cache_read_tokens,
            record.cache_creation_tokens,
            input_cost,
            output_cost,
            cache_read_cost,
            cache_creation_cost,
            total_cost,
            0i64,                  // latency_ms
            Option::<i64>::None,   // first_token_ms
            200i64,                // status_code
            Option::<String>::None,// error_message
            Some(session_id.to_string()),
            Some("kimicode_session"), // provider_type
            1i64,                  // is_streaming
            "1.0",                 // cost_multiplier
            created_at,
            "kimicode_session",    // data_source
        ],
    )
    .map_err(|e| AppError::Database(format!("插入 Kimi Code 会话日志失败: {e}")))?;

    Ok(inserted_rows > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_turn_usage_record() {
        let line = r#"{"type":"usage.record","model":"kimi-code/k3","usage":{"inputOther":1882,"output":184,"inputCacheRead":19200,"inputCacheCreation":0},"usageScope":"turn","time":1786363403330}"#;
        let record = parse_usage_record(line).unwrap();
        assert_eq!(record.model, "kimi-code/k3");
        assert_eq!(record.input_tokens, 1882);
        assert_eq!(record.output_tokens, 184);
        assert_eq!(record.cache_read_tokens, 19200);
        assert_eq!(record.cache_creation_tokens, 0);
        assert_eq!(record.timestamp_ms, 1786363403330);
    }

    #[test]
    fn ignores_other_event_types() {
        assert!(parse_usage_record(r#"{"type":"think","text":"…"}"#).is_none());
        assert!(parse_usage_record(r#"{"type":"step.begin"}"#).is_none());
        assert!(parse_usage_record("not json").is_none());
    }

    #[test]
    fn ignores_non_turn_scope() {
        let line = r#"{"type":"usage.record","model":"m","usage":{"inputOther":1,"output":1},"usageScope":"session","time":1}"#;
        assert!(parse_usage_record(line).is_none());
    }

    #[test]
    fn ignores_all_zero_tokens() {
        let line = r#"{"type":"usage.record","model":"m","usage":{"inputOther":0,"output":0,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1}"#;
        assert!(parse_usage_record(line).is_none());
    }

    #[test]
    fn collect_wire_files_fixed_depth() {
        let root = std::env::temp_dir().join(format!(
            "ccswitch-kimicode-usage-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let wire = root
            .join("sessions/wd_proj_abc/session_1/agents/main/wire.jsonl");
        fs::create_dir_all(wire.parent().unwrap()).unwrap();
        fs::write(&wire, "").unwrap();
        // 深度之外/非 wire 文件不应被收集
        fs::create_dir_all(root.join("sessions/wd_proj_abc/session_1/logs")).unwrap();
        fs::write(root.join("sessions/wd_proj_abc/session_1/state.json"), "{}").unwrap();

        let files = collect_wire_files(&root.join("sessions"));
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].session_id, "session_1");
        assert_eq!(files[0].agent, "main");

        let _ = fs::remove_dir_all(&root);
    }
}
