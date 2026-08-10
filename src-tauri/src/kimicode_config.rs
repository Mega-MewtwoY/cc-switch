//! Kimi Code CLI 配置管理
//!
//! Kimi Code（`kimi`）使用 TOML 配置，默认位于 `~/.kimi-code/config.toml`：
//!
//! ```toml
//! default_model = "kimi-code/kimi-for-coding"
//!
//! [providers."managed:kimi-code"]
//! type = "kimi"
//! api_key = ""
//! base_url = "https://api.kimi.com/coding/v1"
//!
//! [models."kimi-code/kimi-for-coding"]
//! provider = "managed:kimi-code"
//! model = "kimi-for-coding"
//! max_context_size = 262144
//! ```
//!
//! 与 OpenCode 一样是 additive 模式：所有供应商共存于 config.toml，
//! 「当前供应商」由顶层 `default_model` 指针决定。
//!
//! cc-switch 侧每个供应商持久化为 `KimiCodeProviderConfig`（JSON），
//! 写入 live 配置时展开为 `[providers."<id>"]` + 若干 `[models."<id>/<别名>"]` 表。
//! 模型别名规则：别名中已含 `/` 视为全限定名原样使用，否则自动补 `<id>/` 前缀。

use crate::config::{get_home_dir, write_text_file};
use crate::error::AppError;
use crate::provider::KimiCodeProviderConfig;
use crate::settings::get_kimicode_override_dir;
use indexmap::IndexMap;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use toml_edit::{DocumentMut, Item, Table};

fn kimicode_config_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// 获取 Kimi Code 配置目录（默认 ~/.kimi-code，可被设置覆盖）
pub fn get_kimicode_dir() -> PathBuf {
    if let Some(override_dir) = get_kimicode_override_dir() {
        return override_dir;
    }

    get_home_dir().join(".kimi-code")
}

pub fn get_kimicode_config_path() -> PathBuf {
    get_kimicode_dir().join("config.toml")
}

/// 读取 kimi CLI 管理的 OAuth access_token
/// （<kimi_dir>/credentials/kimi-code.json）。
///
/// 官方供应商的 config.toml 里 `api_key` 为空——OAuth 令牌由 kimi CLI
/// 自己持有并刷新，CC Switch 只在需要直接调官方 API（如套餐用量查询）时
/// 从这里借用。文件缺失/损坏/令牌为空时返回 None，由调用方给出引导文案。
pub fn load_oauth_access_token() -> Option<String> {
    load_oauth_access_token_from(&get_kimicode_dir().join("credentials").join("kimi-code.json"))
}

fn load_oauth_access_token_from(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&content).ok()?;
    value
        .get("access_token")?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// 读取 config.toml 为可编辑文档；文件不存在时返回空文档。
///
/// TOML 解析失败会报错而不是重建：config.toml 里还有 thinking/services
/// 等用户自有配置，静默重建等于删掉它们（与 read_claude_live 的做法一致）。
fn read_kimicode_config_from_path(path: &Path) -> Result<DocumentMut, AppError> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(DocumentMut::new());
        }
        Err(err) => return Err(AppError::io(path, err)),
    };

    content.parse::<DocumentMut>().map_err(|e| {
        AppError::Config(format!(
            "Failed to parse Kimi Code config: {}: {e}",
            path.display()
        ))
    })
}

pub fn read_kimicode_config() -> Result<DocumentMut, AppError> {
    read_kimicode_config_from_path(&get_kimicode_config_path())
}

/// 将整个 config.toml 文档转为 JSON（用于 live 配置查看/导入）
pub fn document_to_json(doc: &DocumentMut) -> Value {
    toml_table_to_json(doc.iter())
}

fn write_kimicode_config_to_path(path: &Path, doc: &DocumentMut) -> Result<(), AppError> {
    write_text_file(path, &doc.to_string())?;
    log::debug!("Kimi Code config written to {path:?}");
    Ok(())
}

// ============================================================================
// JSON ↔ TOML 转换
// ============================================================================

/// JSON → toml_edit::Item。TOML 没有 null，返回 None 表示该字段应被跳过。
fn json_to_toml_item(value: &Value) -> Option<Item> {
    match value {
        Value::Null => None,
        Value::Bool(b) => Some(Item::Value((*b).into())),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(Item::Value(i.into()))
            } else {
                n.as_f64().map(|f| Item::Value(f.into()))
            }
        }
        Value::String(s) => Some(Item::Value(s.as_str().into())),
        Value::Array(arr) => {
            let mut toml_arr = toml_edit::Array::new();
            for item in arr {
                match json_to_toml_item(item) {
                    Some(Item::Value(v)) => toml_arr.push(v),
                    // TOML 数组不支持嵌套表以外的 Item，null 跳过
                    Some(_) | None => continue,
                }
            }
            Some(Item::Value(toml_arr.into()))
        }
        Value::Object(obj) => {
            let mut table = Table::new();
            for (key, val) in obj {
                if let Some(item) = json_to_toml_item(val) {
                    table.insert(key, item);
                }
            }
            Some(Item::Table(table))
        }
    }
}

/// toml_edit::Value → JSON
fn toml_value_to_json(v: &toml_edit::Value) -> Value {
    match v {
        toml_edit::Value::String(s) => Value::String(s.value().clone()),
        toml_edit::Value::Integer(i) => Value::Number((*i.value()).into()),
        toml_edit::Value::Float(f) => serde_json::Number::from_f64(*f.value())
            .map(Value::Number)
            .unwrap_or(Value::Null),
        toml_edit::Value::Boolean(b) => Value::Bool(*b.value()),
        toml_edit::Value::Datetime(d) => Value::String(d.to_string()),
        toml_edit::Value::Array(arr) => Value::Array(arr.iter().map(toml_value_to_json).collect()),
        toml_edit::Value::InlineTable(t) => {
            let mut map = Map::new();
            for (key, val) in t.iter() {
                map.insert(key.to_string(), toml_value_to_json(val));
            }
            Value::Object(map)
        }
    }
}

/// toml_edit::Item → JSON（用于从 live 配置重建 cc-switch 供应商结构）
fn toml_item_to_json(item: &Item) -> Value {
    match item {
        Item::None => Value::Null,
        Item::Value(v) => toml_value_to_json(v),
        Item::Table(t) => toml_table_to_json(t.iter()),
        Item::ArrayOfTables(arr) => Value::Array(
            arr.iter()
                .map(|t| toml_table_to_json(t.iter()))
                .collect(),
        ),
    }
}

fn toml_table_to_json<'a>(entries: impl Iterator<Item = (&'a str, &'a Item)>) -> Value {
    let mut map = Map::new();
    for (key, item) in entries {
        map.insert(key.to_string(), toml_item_to_json(item));
    }
    Value::Object(map)
}

// ============================================================================
// 模型别名规则
// ============================================================================

/// 别名 → config.toml 中的完整模型键：已含 `/` 视为全限定名，否则补 `<provider_id>/` 前缀。
fn qualify_model_key(provider_id: &str, alias: &str) -> String {
    if alias.contains('/') {
        alias.to_string()
    } else {
        format!("{provider_id}/{alias}")
    }
}

/// config.toml 模型键 → cc-switch 侧别名：剥离 `<provider_id>/` 前缀（若有）。
fn strip_model_key_prefix<'a>(provider_id: &str, key: &'a str) -> &'a str {
    key.strip_prefix(provider_id)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or(key)
}

// ============================================================================
// Provider 读写
// ============================================================================

/// 从 live 配置重建所有供应商（含官方 OAuth 供应商，如 managed:kimi-code）。
///
/// 供应商与模型的归属关系由模型表中的 `provider` 字段决定（与官方约定一致），
/// 而不是键前缀——官方配置的模型键（kimi-code/...）就不以供应商 id 开头。
pub fn get_providers() -> Result<IndexMap<String, KimiCodeProviderConfig>, AppError> {
    let doc = read_kimicode_config()?;
    let mut result = IndexMap::new();

    let Some(providers_table) = doc.get("providers").and_then(Item::as_table) else {
        return Ok(result);
    };

    let default_model = doc.get("default_model").and_then(Item::as_str);

    for (provider_id, provider_item) in providers_table.iter() {
        let spec_json = toml_item_to_json(provider_item);
        let spec = match serde_json::from_value(spec_json) {
            Ok(spec) => spec,
            Err(e) => {
                log::warn!("Failed to parse Kimi Code provider '{provider_id}': {e}");
                continue;
            }
        };

        // 收集归属该供应商的模型
        let mut models = std::collections::HashMap::new();
        if let Some(models_table) = doc.get("models").and_then(Item::as_table) {
            for (model_key, model_item) in models_table.iter() {
                let belongs = model_item
                    .get("provider")
                    .and_then(Item::as_str)
                    .map(|p| p == provider_id)
                    .unwrap_or(false);
                if !belongs {
                    continue;
                }
                let mut model_json = toml_item_to_json(model_item);
                // provider 字段是 TOML 侧的归属指针，cc-switch 侧不需要
                if let Value::Object(ref mut obj) = model_json {
                    obj.remove("provider");
                }
                match serde_json::from_value(model_json) {
                    Ok(model) => {
                        let alias = strip_model_key_prefix(provider_id, model_key).to_string();
                        models.insert(alias, model);
                    }
                    Err(e) => {
                        log::warn!("Failed to parse Kimi Code model '{model_key}': {e}");
                    }
                }
            }
        }

        let provider_default = default_model.and_then(|dm| {
            let alias = strip_model_key_prefix(provider_id, dm);
            // default_model 必须确实指向该供应商的某个模型
            models.contains_key(alias).then(|| alias.to_string())
        });

        result.insert(
            provider_id.to_string(),
            KimiCodeProviderConfig {
                provider: spec,
                models,
                default_model: provider_default,
            },
        );
    }

    Ok(result)
}

/// 写入/更新一个供应商：展开为 `[providers."<id>"]` + `[models."<id>/<别名>"]` 表。
/// 不影响其他供应商，也不动 `default_model`（当前供应商切换走 set_default_model）。
pub fn set_provider(id: &str, config: &KimiCodeProviderConfig) -> Result<(), AppError> {
    let _guard = kimicode_config_lock().lock()?;
    let path = get_kimicode_config_path();
    let mut doc = read_kimicode_config_from_path(&path)?;

    // providers 段是投影区之一，但 thinking/services 等用户自有配置必须保留，
    // 因此只在 doc 上定点修改，绝不整体重写。
    // 注意：toml_edit 的链式索引（doc["providers"][id] = …）会把缺失键建成
    // 隐式 InlineTable 导致写入丢失，必须先显式创建 Table 再 insert。
    if !matches!(doc.get("providers"), Some(Item::Table(_))) {
        if doc.get("providers").is_some() {
            log::warn!("config.toml 的 providers 不是表，已重置");
        }
        doc["providers"] = Item::Table(Table::new());
    }
    if !matches!(doc.get("models"), Some(Item::Table(_))) {
        if doc.get("models").is_some() {
            log::warn!("config.toml 的 models 不是表，已重置");
        }
        doc["models"] = Item::Table(Table::new());
    }

    // 1. 写入供应商表
    let spec_value = serde_json::to_value(&config.provider)
        .map_err(|e| AppError::JsonSerialize { source: e })?;
    let provider_item = json_to_toml_item(&spec_value)
        .ok_or_else(|| AppError::Config(format!("Kimi Code 供应商 '{id}' 配置为空")))?;
    if let Some(t) = doc.get_mut("providers").and_then(Item::as_table_mut) {
        t.insert(id, provider_item);
    }

    // 2. 先移除该供应商名下、但不在新模型集合中的旧模型表
    let new_keys: std::collections::HashSet<String> = config
        .models
        .keys()
        .map(|alias| qualify_model_key(id, alias))
        .collect();
    if let Some(t) = doc.get_mut("models").and_then(Item::as_table_mut) {
        let stale_keys: Vec<String> = t
            .iter()
            .filter(|(key, item)| {
                item.get("provider").and_then(Item::as_str) == Some(id)
                    && !new_keys.contains(*key)
            })
            .map(|(key, _)| key.to_string())
            .collect();
        for key in stale_keys {
            t.remove(&key);
        }
    }

    // 3. 写入模型表（附带 provider 归属指针）
    for (alias, model) in &config.models {
        let key = qualify_model_key(id, alias);
        let mut model_value = serde_json::to_value(model)
            .map_err(|e| AppError::JsonSerialize { source: e })?;
        if let Value::Object(ref mut obj) = model_value {
            obj.insert("provider".to_string(), Value::String(id.to_string()));
        }
        if let Some(item) = json_to_toml_item(&model_value) {
            if let Some(t) = doc.get_mut("models").and_then(Item::as_table_mut) {
                t.insert(&key, item);
            }
        }
    }

    write_kimicode_config_to_path(&path, &doc)
}

/// 移除供应商：删除其 providers 表和名下所有模型表；
/// 若 default_model 指向被删模型，一并清除（避免悬空指针让 kimi 启动报错）。
pub fn remove_provider(id: &str) -> Result<(), AppError> {
    let _guard = kimicode_config_lock().lock()?;
    let path = get_kimicode_config_path();
    let mut doc = read_kimicode_config_from_path(&path)?;

    if let Some(t) = doc.get_mut("providers").and_then(Item::as_table_mut) {
        t.remove(id);
    }

    if let Some(t) = doc.get_mut("models").and_then(Item::as_table_mut) {
        let owned_keys: Vec<String> = t
            .iter()
            .filter(|(_, item)| item.get("provider").and_then(Item::as_str) == Some(id))
            .map(|(key, _)| key.to_string())
            .collect();
        for key in &owned_keys {
            t.remove(key);
        }

        // 清理悬空的 default_model
        if let Some(dm) = doc.get("default_model").and_then(Item::as_str) {
            if owned_keys.iter().any(|k| k == dm) {
                doc.remove("default_model");
            }
        }
    }

    write_kimicode_config_to_path(&path, &doc)
}

// ============================================================================
// 当前供应商（default_model 指针）
// ============================================================================

/// 读取顶层 default_model（全限定模型键）
pub fn get_default_model() -> Result<Option<String>, AppError> {
    let doc = read_kimicode_config()?;
    Ok(doc
        .get("default_model")
        .and_then(Item::as_str)
        .map(|s| s.to_string()))
}

/// 设置顶层 default_model
pub fn set_default_model(model_key: &str) -> Result<(), AppError> {
    let _guard = kimicode_config_lock().lock()?;
    let path = get_kimicode_config_path();
    let mut doc = read_kimicode_config_from_path(&path)?;
    doc["default_model"] = toml_edit::value(model_key);
    write_kimicode_config_to_path(&path, &doc)
}

/// 把 default_model 指向指定供应商的某个模型别名（自动补全限定前缀）
pub fn set_default_model_for(provider_id: &str, alias: &str) -> Result<(), AppError> {
    set_default_model(&qualify_model_key(provider_id, alias))
}

/// 从 live 配置推断「当前供应商 id」：default_model 指向的模型属于哪个供应商。
pub fn resolve_current_provider_id() -> Result<Option<String>, AppError> {
    let Some(default_model) = get_default_model()? else {
        return Ok(None);
    };
    let doc = read_kimicode_config()?;
    Ok(doc
        .get("models")
        .and_then(|m| m.get(&default_model))
        .and_then(|m| m.get("provider"))
        .and_then(Item::as_str)
        .map(|s| s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{KimiCodeModel, KimiCodeProviderSpec};

    struct TestHomeGuard(Option<std::ffi::OsString>);
    impl TestHomeGuard {
        fn set(home: &std::path::Path) -> Self {
            let guard = Self(std::env::var_os("CC_SWITCH_TEST_HOME"));
            std::env::set_var("CC_SWITCH_TEST_HOME", home);
            guard
        }
    }
    impl Drop for TestHomeGuard {
        fn drop(&mut self) {
            match self.0.take() {
                Some(value) => std::env::set_var("CC_SWITCH_TEST_HOME", value),
                None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
            }
        }
    }

    fn write_config(home: &std::path::Path, content: &str) {
        let dir = home.join(".kimi-code");
        std::fs::create_dir_all(&dir).expect("create config dir");
        std::fs::write(dir.join("config.toml"), content).expect("write config");
    }

    fn sample_provider() -> KimiCodeProviderConfig {
        let mut models = std::collections::HashMap::new();
        models.insert(
            "kimi-k2.6".to_string(),
            KimiCodeModel {
                model: "kimi-k2.6".to_string(),
                max_context_size: Some(262144),
                capabilities: Some(vec!["thinking".to_string(), "tool_use".to_string()]),
                display_name: Some("Kimi K2.6".to_string()),
                extra: std::collections::HashMap::new(),
            },
        );
        KimiCodeProviderConfig {
            provider: KimiCodeProviderSpec {
                provider_type: "openai".to_string(),
                api_key: Some("sk-test".to_string()),
                base_url: Some("https://api.example.com/v1".to_string()),
                extra: std::collections::HashMap::new(),
            },
            models,
            default_model: Some("kimi-k2.6".to_string()),
        }
    }

    #[test]
    #[serial_test::serial]
    fn set_and_get_provider_roundtrip() {
        let temp = tempfile::tempdir().expect("tempdir");
        let _guard = TestHomeGuard::set(temp.path());

        set_provider("my-proxy", &sample_provider()).expect("set provider");

        let providers = get_providers().expect("get providers");
        let config = providers.get("my-proxy").expect("provider exists");
        assert_eq!(config.provider.provider_type, "openai");
        assert_eq!(
            config.provider.api_key.as_deref(),
            Some("sk-test"),
            "api_key must round-trip"
        );
        assert!(
            config.models.contains_key("kimi-k2.6"),
            "model alias must be stripped of the provider prefix"
        );
        assert!(config.default_model.is_none(), "default_model 不应由 set_provider 设置");

        set_default_model("my-proxy/kimi-k2.6").expect("set default model");
        let providers = get_providers().expect("reload");
        assert_eq!(
            providers["my-proxy"].default_model.as_deref(),
            Some("kimi-k2.6")
        );
        assert_eq!(
            resolve_current_provider_id().expect("resolve"),
            Some("my-proxy".to_string())
        );
    }

    #[test]
    #[serial_test::serial]
    fn set_provider_preserves_unrelated_config() {
        let temp = tempfile::tempdir().expect("tempdir");
        let _guard = TestHomeGuard::set(temp.path());
        write_config(
            temp.path(),
            r#"default_model = "kimi-code/kimi-for-coding"

[providers."managed:kimi-code"]
type = "kimi"
api_key = ""

[models."kimi-code/kimi-for-coding"]
provider = "managed:kimi-code"
model = "kimi-for-coding"

[thinking]
enabled = true
"#,
        );

        set_provider("my-proxy", &sample_provider()).expect("set provider");

        let content = std::fs::read_to_string(get_kimicode_config_path()).expect("read");
        assert!(content.contains("[thinking]"), "用户自有配置必须保留");
        assert!(content.contains("managed:kimi-code"), "官方供应商必须保留");

        let providers = get_providers().expect("get providers");
        assert!(providers.contains_key("managed:kimi-code"));
        assert_eq!(
            providers["managed:kimi-code"].default_model.as_deref(),
            Some("kimi-code/kimi-for-coding"),
            "官方模型的全限定键不含供应商前缀时，别名应保留原样"
        );
    }

    #[test]
    #[serial_test::serial]
    fn remove_provider_cleans_models_and_dangling_default() {
        let temp = tempfile::tempdir().expect("tempdir");
        let _guard = TestHomeGuard::set(temp.path());

        set_provider("my-proxy", &sample_provider()).expect("set provider");
        set_default_model("my-proxy/kimi-k2.6").expect("set default");

        remove_provider("my-proxy").expect("remove provider");

        let providers = get_providers().expect("get providers");
        assert!(!providers.contains_key("my-proxy"));
        assert_eq!(
            get_default_model().expect("default model"),
            None,
            "悬空的 default_model 必须被清除"
        );
    }

    #[test]
    #[serial_test::serial]
    fn read_rejects_invalid_toml_instead_of_rebuilding() {
        let temp = tempfile::tempdir().expect("tempdir");
        let _guard = TestHomeGuard::set(temp.path());
        write_config(temp.path(), "this is [not valid toml");

        assert!(
            read_kimicode_config().is_err(),
            "损坏的 TOML 必须报错而不是静默重建"
        );
    }

    #[test]
    fn load_oauth_access_token_reads_credentials_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("kimi-code.json");
        std::fs::write(
            &path,
            r#"{"access_token":"tok-abc","refresh_token":"tok-refresh","expires_at":1893456000}"#,
        )
        .expect("write credentials");

        assert_eq!(
            load_oauth_access_token_from(&path).as_deref(),
            Some("tok-abc")
        );
    }

    #[test]
    fn load_oauth_access_token_returns_none_when_unusable() {
        let temp = tempfile::tempdir().expect("tempdir");
        let missing = temp.path().join("missing.json");
        assert_eq!(load_oauth_access_token_from(&missing), None);

        let empty_token = temp.path().join("empty.json");
        std::fs::write(&empty_token, r#"{"access_token":""}"#).expect("write");
        assert_eq!(load_oauth_access_token_from(&empty_token), None);

        let invalid = temp.path().join("invalid.json");
        std::fs::write(&invalid, "not json").expect("write");
        assert_eq!(load_oauth_access_token_from(&invalid), None);
    }
}
