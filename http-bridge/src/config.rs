use codex_login::AuthCredentialsStoreMode;
use codex_login::AuthKeyringBackendKind;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HttpBridgeConfig {
    pub codex_home: PathBuf,
    /// Override the Responses base URL. By default ChatGPT auth selects the official Codex URL.
    pub base_url: Option<String>,
    pub credentials_store_mode: AuthCredentialsStoreMode,
    pub keyring_backend_kind: AuthKeyringBackendKind,
    pub request_compression: bool,
    pub beta_features: Option<String>,
}

impl Default for HttpBridgeConfig {
    fn default() -> Self {
        Self {
            codex_home: default_codex_home(),
            base_url: None,
            credentials_store_mode: AuthCredentialsStoreMode::File,
            keyring_backend_kind: AuthKeyringBackendKind::default(),
            request_compression: true,
            beta_features: None,
        }
    }
}

fn default_codex_home() -> PathBuf {
    if let Some(path) = std::env::var_os("CODEX_HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(path);
    }
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codex")
}

impl HttpBridgeConfig {
    pub fn from_json(json: &[u8]) -> Result<Self, serde_json::Error> {
        if json.is_empty() {
            return Ok(Self::default());
        }
        serde_json::from_slice(json)
    }
}
