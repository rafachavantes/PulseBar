use super::*;

// ── Credential detection ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiCliStatus {
    pub signed_in: bool,
    pub credentials_path: Option<String>,
}

fn gemini_cli_credentials_path() -> Option<std::path::PathBuf> {
    pulsebar::host::session::gemini_cli_credentials_path()
}

#[tauri::command]
pub fn get_gemini_cli_signed_in() -> Result<GeminiCliStatus, String> {
    let path = gemini_cli_credentials_path();
    let signed_in = path.as_ref().map(|p| p.exists()).unwrap_or(false);
    Ok(GeminiCliStatus {
        signed_in,
        credentials_path: path.map(|p| p.to_string_lossy().into_owned()),
    })
}
