use crate::BridgeError;
use codex_login::AuthCredentialsStoreMode;
use codex_login::AuthKeyringBackendKind;
use codex_login::AuthManager;
use codex_login::RefreshTokenError;
use serde_json::Value;
use serde_json::json;

/// Refreshes one stored Codex `auth.json` payload using the upstream refresh flow.
///
/// Embedding proxies keep credentials in their own storage rather than a Codex home, so the
/// payload is materialized into a temporary Codex home, refreshed through the upstream
/// [`AuthManager`] (which rotates the refresh token and persists the result), then read back.
///
/// The returned JSON always describes the outcome so callers can distinguish a retryable
/// failure from one that requires a new browser login:
///
/// ```text
/// {"refreshed": true,  "permanent": false, "message": "",    "auth": { ... }}
/// {"refreshed": false, "permanent": true,  "message": "...", "auth": null}
/// ```
pub fn refresh_credentials(auth_json: &[u8]) -> Result<Value, BridgeError> {
    serde_json::from_slice::<Value>(auth_json).map_err(BridgeError::InvalidConfig)?;

    let temp = tempfile::tempdir().map_err(BridgeError::Login)?;
    let home = temp.path().to_path_buf();
    std::fs::write(home.join("auth.json"), auth_json).map_err(BridgeError::Login)?;

    let runtime = tokio::runtime::Runtime::new().map_err(BridgeError::Runtime)?;
    let auth_path = home.join("auth.json");
    runtime.block_on(async move {
        let manager = AuthManager::new(
            home,
            /*enable_codex_api_key_env*/ false,
            AuthCredentialsStoreMode::File,
            /*forced_chatgpt_workspace_id*/ None,
            /*chatgpt_base_url*/ None,
            AuthKeyringBackendKind::default(),
            /*auth_route_config*/ crate::default_auth_route_config(),
        )
        .await;

        match manager.refresh_token().await {
            Ok(()) => {
                let refreshed = std::fs::read(&auth_path).map_err(BridgeError::Login)?;
                let auth: Value =
                    serde_json::from_slice(&refreshed).map_err(BridgeError::InvalidConfig)?;
                Ok(json!({
                    "refreshed": true,
                    "permanent": false,
                    "message": "",
                    "auth": auth,
                }))
            }
            // Permanent failures mean the refresh token itself is expired, already used or
            // revoked; only a new browser login can recover.
            Err(RefreshTokenError::Permanent(failed)) => Ok(json!({
                "refreshed": false,
                "permanent": true,
                "message": failed.to_string(),
                "auth": Value::Null,
            })),
            Err(RefreshTokenError::Transient(error)) => Ok(json!({
                "refreshed": false,
                "permanent": false,
                "message": error.to_string(),
                "auth": Value::Null,
            })),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_auth_json_before_network() {
        let error = refresh_credentials(b"{").expect_err("invalid JSON must fail");
        assert!(error.to_string().contains("invalid login configuration"));
    }
}
