use codex_login::AuthCredentialsStoreMode;
use codex_login::AuthKeyringBackendKind;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

/// Configuration for a ChatGPT browser login.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoginConfig {
    /// Codex state directory where auth.json is persisted when file storage is selected.
    pub codex_home: PathBuf,
    /// Open the system browser after starting the local OAuth callback server.
    pub open_browser: bool,
    /// Preferred callback port. Set to 0 to ask the OS for an ephemeral port.
    pub port: u16,
    /// OAuth issuer. The production Codex default is https://auth.openai.com.
    pub issuer: Option<String>,
    /// Optional ChatGPT workspace allow-list.
    pub forced_chatgpt_workspace_id: Option<Vec<String>>,
    /// Credential persistence backend. The bridge defaults to file for portability.
    pub credentials_store_mode: AuthCredentialsStoreMode,
    /// Keyring implementation used when credentials_store_mode requires a keyring.
    pub keyring_backend_kind: AuthKeyringBackendKind,
    /// Revoke and remove an existing login before beginning a new browser login.
    pub clear_existing: bool,
}

impl Default for LoginConfig {
    fn default() -> Self {
        Self {
            codex_home: default_codex_home(),
            open_browser: true,
            port: 1455,
            issuer: None,
            forced_chatgpt_workspace_id: None,
            credentials_store_mode: AuthCredentialsStoreMode::File,
            keyring_backend_kind: AuthKeyringBackendKind::default(),
            clear_existing: true,
        }
    }
}

impl LoginConfig {
    pub fn from_json(json: &[u8]) -> Result<Self, serde_json::Error> {
        if json.is_empty() {
            return Ok(Self::default());
        }
        serde_json::from_slice(json)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_json_uses_defaults() {
        let config = LoginConfig::from_json(b"").expect("parse defaults");
        assert!(config.open_browser);
        assert_eq!(config.port, 1455);
        assert_eq!(
            config.credentials_store_mode,
            AuthCredentialsStoreMode::File
        );
    }

    #[test]
    fn parses_partial_json() {
        let config =
            LoginConfig::from_json(br#"{"open_browser":false,"port":0}"#).expect("parse config");
        assert!(!config.open_browser);
        assert_eq!(config.port, 0);
    }
}
