use super::*;

/// API key storage for providers that need tokens
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiKeys {
    /// Provider ID -> API key mapping
    pub keys: HashMap<String, ApiKeyEntry>,
}

/// A single API key entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyEntry {
    pub api_key: String,
    pub saved_at: String,
    /// Optional label for the key (e.g., "Personal", "Work")
    #[serde(default)]
    pub label: Option<String>,
}

impl ApiKeys {
    /// Get the API keys file path
    pub fn keys_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("PulseBar").join("api_keys.json"))
    }

    /// Load API keys from disk
    pub fn load() -> Self {
        if let Some(path) = Self::keys_path()
            && path.exists()
            && let Ok(content) = crate::secure_file::read_string(&path)
        {
            return serde_json::from_str(&content).unwrap_or_default();
        }
        Self::default()
    }

    /// Save API keys to disk
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::keys_path()
            .ok_or_else(|| anyhow::anyhow!("Could not determine API keys path"))?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)?;
        crate::secure_file::write_string(&path, &json)?;

        Ok(())
    }

    /// Get API key for a provider
    pub fn get(&self, provider_id: &str) -> Option<&str> {
        self.keys.get(provider_id).map(|e| e.api_key.as_str())
    }

    /// Set API key for a provider
    pub fn set(&mut self, provider_id: &str, api_key: &str, label: Option<&str>) {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M").to_string();
        self.keys.insert(
            provider_id.to_string(),
            ApiKeyEntry {
                api_key: api_key.to_string(),
                saved_at: now,
                label: label.map(|s| s.to_string()),
            },
        );
    }

    /// Remove API key for a provider
    pub fn remove(&mut self, provider_id: &str) {
        self.keys.remove(provider_id);
    }

    /// Check if a provider has an API key configured
    pub fn has_key(&self, provider_id: &str) -> bool {
        self.keys
            .get(provider_id)
            .map(|e| !e.api_key.is_empty())
            .unwrap_or(false)
    }

    /// Get all saved API keys for UI display (with masked values)
    pub fn get_all_for_display(&self) -> Vec<SavedApiKeyInfo> {
        self.keys
            .iter()
            .map(|(id, entry)| {
                let provider_name = ProviderId::from_cli_name(id)
                    .map(|p| p.display_name().to_string())
                    .unwrap_or_else(|| id.clone());

                // Mask the key for display (show first 4 and last 4 chars)
                let masked = if entry.api_key.len() > 12 {
                    format!(
                        "{}...{}",
                        &entry.api_key[..4],
                        &entry.api_key[entry.api_key.len() - 4..]
                    )
                } else if entry.api_key.len() > 4 {
                    format!("{}...", &entry.api_key[..4])
                } else {
                    "****".to_string()
                };

                SavedApiKeyInfo {
                    provider_id: id.clone(),
                    provider: provider_name,
                    masked_key: masked,
                    saved_at: entry.saved_at.clone(),
                    label: entry.label.clone(),
                }
            })
            .collect()
    }
}

/// Info about a saved API key for UI display
#[derive(Debug, Clone, Serialize)]
pub struct SavedApiKeyInfo {
    pub provider_id: String,
    pub provider: String,
    pub masked_key: String,
    pub saved_at: String,
    pub label: Option<String>,
}

/// Provider configuration info
#[derive(Debug, Clone)]
pub struct ProviderConfigInfo {
    pub id: ProviderId,
    pub name: &'static str,
    pub requires_api_key: bool,
    pub api_key_env_var: Option<&'static str>,
    pub api_key_help: Option<&'static str>,
    pub config_file_path: Option<&'static str>,
    pub dashboard_url: Option<&'static str>,
}

/// Get configuration info for providers that need API keys
pub fn get_api_key_providers() -> Vec<ProviderConfigInfo> {
    vec![
        ProviderConfigInfo {
            id: ProviderId::Synthetic,
            name: "Synthetic",
            requires_api_key: true,
            api_key_env_var: Some("SYNTHETIC_API_KEY"),
            api_key_help: Some("Get your API key from Synthetic → Account → API Keys"),
            config_file_path: Some("~/.synthetic/config.json"),
            dashboard_url: Some("https://synthetic.computer/account"),
        },
        ProviderConfigInfo {
            id: ProviderId::Zai,
            name: "z.ai",
            requires_api_key: true,
            api_key_env_var: Some("ZAI_API_TOKEN"),
            api_key_help: Some("Get your API token from z.ai Dashboard → Settings"),
            config_file_path: None,
            dashboard_url: Some("https://z.ai/dashboard"),
        },
        ProviderConfigInfo {
            id: ProviderId::Grok,
            name: "Grok",
            requires_api_key: false,
            api_key_env_var: None,
            api_key_help: Some("Uses Grok browser cookies or ~/.grok/auth.json."),
            config_file_path: Some("~/.grok/auth.json"),
            dashboard_url: Some("https://grok.com/settings/subscription"),
        },
    ]
}
