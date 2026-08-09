use crate::BridgeError;
use codex_http_client::HttpClientFactory;
use codex_http_client::OutboundProxyPolicy;
use codex_login::AuthCredentialsStoreMode;
use codex_login::AuthKeyringBackendKind;
use codex_login::AuthManager;
use codex_model_provider::create_model_provider;
use codex_model_provider_info::ModelProviderInfo;
use codex_models_manager::manager::RefreshStrategy;
use serde_json::Value;
use serde_json::json;
use std::sync::Arc;

/// Lists the models available to one stored ChatGPT credential.
///
/// This mirrors what the Codex CLI `/model` picker shows: the catalog is fetched through the
/// upstream models manager, which sorts by priority, filters by the account's auth mode and
/// marks the default. Each returned entry carries its own supported reasoning efforts and
/// service tiers, so the caller must re-read those after the selected model changes.
///
/// The payload is the serialized upstream `ModelPreset` list, wrapped as:
///
/// ```text
/// {"models": [ { "id", "model", "display_name", "default_reasoning_effort",
///                "supported_reasoning_efforts": [...], "service_tiers": [...],
///                "default_service_tier", "is_default", "show_in_picker" }, ... ]}
/// ```
pub fn list_models(auth_json: &[u8]) -> Result<Value, BridgeError> {
    serde_json::from_slice::<Value>(auth_json).map_err(BridgeError::InvalidConfig)?;

    // The catalog is auth-filtered, so materialize the credential into a throwaway Codex home
    // exactly like the refresh path does. The cache file lands there too and is discarded, which
    // means every call re-fetches; callers should cache the result themselves.
    let temp = tempfile::tempdir().map_err(BridgeError::Login)?;
    let home = temp.path().to_path_buf();
    std::fs::write(home.join("auth.json"), auth_json).map_err(BridgeError::Login)?;

    let runtime = tokio::runtime::Runtime::new().map_err(BridgeError::Runtime)?;
    runtime.block_on(async move {
        let auth_manager = Arc::new(
            AuthManager::new(
                home.clone(),
                /*enable_codex_api_key_env*/ false,
                AuthCredentialsStoreMode::File,
                /*forced_chatgpt_workspace_id*/ None,
                /*chatgpt_base_url*/ None,
                AuthKeyringBackendKind::default(),
                /*auth_route_config*/ crate::default_auth_route_config(),
            )
            .await,
        );
        if auth_manager.auth().await.is_none() {
            return Err(BridgeError::NoCredentials);
        }

        let mut provider_info = ModelProviderInfo::create_openai_provider(None);
        provider_info.requires_openai_auth = true;
        let provider = create_model_provider(provider_info, Some(Arc::clone(&auth_manager)));
        let manager = provider.models_manager(home, /*config_model_catalog*/ None);

        let factory = HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault);
        let presets = manager
            .list_models(RefreshStrategy::OnlineIfUncached, factory)
            .await;

        Ok(json!({ "models": presets }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_auth_json_before_network() {
        let error = list_models(b"{").expect_err("invalid JSON must fail");
        assert!(error.to_string().contains("invalid login configuration"));
    }
}
