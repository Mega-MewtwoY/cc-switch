use tauri::State;

use crate::kimicode_config;
use crate::store::AppState;

// ============================================================================
// Kimi Code Provider Commands
// ============================================================================

/// Import providers from Kimi Code live config to database.
///
/// Kimi Code uses additive mode — users may already have providers
/// configured in config.toml (including the official OAuth provider).
#[tauri::command]
pub fn import_kimicode_providers_from_live(state: State<'_, AppState>) -> Result<usize, String> {
    crate::services::provider::import_kimicode_providers_from_live(state.inner())
        .map_err(|e| e.to_string())
}

/// Get provider ids in the Kimi Code live config.
#[tauri::command]
pub fn get_kimicode_live_provider_ids() -> Result<Vec<String>, String> {
    kimicode_config::get_providers()
        .map(|providers| providers.keys().cloned().collect())
        .map_err(|e| e.to_string())
}
