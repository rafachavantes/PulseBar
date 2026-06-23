//! Provider implementations

#![allow(dead_code)]

pub mod claude;
pub mod codex;
pub mod gemini;
pub mod grok;
pub mod synthetic;
pub mod zai;

// Re-export provider implementations
pub use claude::ClaudeProvider;
pub use codex::CodexProvider;
pub use gemini::GeminiProvider;
pub use grok::GrokProvider;
pub use synthetic::SyntheticProvider;
pub use zai::ZaiProvider;

pub(crate) fn resolve_api_key(
    explicit: Option<&str>,
    credential_target: &str,
    env_names: &[&str],
) -> Result<String, crate::core::ProviderError> {
    if let Some(key) = explicit
        && !key.trim().is_empty()
    {
        return Ok(key.trim().to_string());
    }
    if let Ok(entry) = keyring::Entry::new(credential_target, "api_key")
        && let Ok(key) = entry.get_password()
        && !key.trim().is_empty()
    {
        return Ok(key);
    }
    for env in env_names {
        if let Ok(key) = std::env::var(env)
            && !key.trim().is_empty()
        {
            return Ok(key);
        }
    }
    Err(crate::core::ProviderError::NotInstalled(format!(
        "API key not found. Set {} in Preferences or environment.",
        env_names.join(" / ")
    )))
}

pub(crate) fn validated_https_url(
    raw: &str,
    label: &str,
) -> Result<reqwest::Url, crate::core::ProviderError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(crate::core::ProviderError::Other(format!(
            "{label} URL is empty"
        )));
    }
    let lower = trimmed.to_ascii_lowercase();
    if ["%2f", "%5c", "%3f", "%23", "%40", "%3a"]
        .iter()
        .any(|encoded| lower.contains(encoded))
    {
        return Err(crate::core::ProviderError::Other(format!(
            "{label} URL must not contain encoded host delimiters"
        )));
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let url = reqwest::Url::parse(&candidate)
        .map_err(|e| crate::core::ProviderError::Other(format!("Invalid {label} URL: {e}")))?;
    let host = url.host_str().ok_or_else(|| {
        crate::core::ProviderError::Other(format!("{label} URL must include a host"))
    })?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || host.contains('%')
        || host.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(crate::core::ProviderError::Other(format!(
            "{label} URL must use HTTPS without user info or encoded host tricks"
        )));
    }
    Ok(url)
}
