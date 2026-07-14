//! Canonical provider factory.
//!
//! All call sites (CLI, Tauri desktop shell, legacy native UI) must construct
//! provider instances through [`instantiate`] so that adding a new provider
//! only requires editing `ProviderId`, the matching `providers/` module, and
//! this one match arm.

use super::{Provider, ProviderId};
use crate::providers::{
    ClaudeProvider, CodexProvider, GeminiProvider, GrokProvider, OpenCodeGoProvider,
    SyntheticProvider, ZaiProvider,
};

/// Instantiate the concrete [`Provider`] implementation for a given [`ProviderId`].
///
/// Exhaustive over [`ProviderId`]: adding a new variant is a compile error until
/// the corresponding provider type is wired in below.
pub fn instantiate(id: ProviderId) -> Box<dyn Provider> {
    match id {
        ProviderId::Codex => Box::new(CodexProvider::new()),
        ProviderId::Claude => Box::new(ClaudeProvider::new()),
        ProviderId::Gemini => Box::new(GeminiProvider::new()),
        ProviderId::Zai => Box::new(ZaiProvider::new()),
        ProviderId::Grok => Box::new(GrokProvider::new()),
        ProviderId::OpenCodeGo => Box::new(OpenCodeGoProvider::new()),
        ProviderId::Synthetic => Box::new(SyntheticProvider::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_provider_id_is_instantiable() {
        for &id in ProviderId::all() {
            let provider = instantiate(id);
            assert_eq!(
                provider.id(),
                id,
                "factory returned wrong provider for {id}"
            );
        }
    }
}
