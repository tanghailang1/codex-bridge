use crate::BridgeError;
use crate::LoginConfig;
use codex_login::AuthDotJson;
use codex_login::CLIENT_ID;
use codex_login::LoginServer;
use codex_login::ServerOptions;
use codex_login::ShutdownHandle;
use codex_login::load_auth_dot_json;
use codex_login::logout_with_revoke;
use codex_login::run_login_server;
use serde::Serialize;
use std::sync::Mutex;
use tokio::runtime::Runtime;

#[derive(Debug, Clone, Serialize)]
pub struct LoginStartInfo {
    pub auth_url: String,
    pub callback_port: u16,
    pub codex_home: String,
}

/// Full Codex credential payload. Treat the serialized value as a secret.
#[derive(Debug, Clone, Serialize)]
pub struct CredentialExport {
    pub codex_home: String,
    pub auth: AuthDotJson,
}

/// A running Codex ChatGPT browser login.
pub struct LoginSession {
    config: LoginConfig,
    runtime: Runtime,
    server: Mutex<Option<LoginServer>>,
    cancel_handle: ShutdownHandle,
    start_info: LoginStartInfo,
}

impl LoginSession {
    pub fn start_info(&self) -> &LoginStartInfo {
        &self.start_info
    }

    /// Wait for the browser callback and return the persisted credentials.
    ///
    /// This may only be called once for a session.
    pub fn wait(&self) -> Result<CredentialExport, BridgeError> {
        let server = self
            .server
            .lock()
            .map_err(|_| BridgeError::Login(std::io::Error::other("login mutex poisoned")))?
            .take()
            .ok_or(BridgeError::AlreadyWaited)?;

        self.runtime
            .block_on(server.block_until_done())
            .map_err(BridgeError::Login)?;
        export_credentials(&self.config)
    }

    pub fn cancel(&self) {
        self.cancel_handle.shutdown();
    }
}

/// Start the exact browser OAuth flow implemented by the vendored Codex login crate.
pub fn start_login(config: LoginConfig) -> Result<LoginSession, BridgeError> {
    std::fs::create_dir_all(&config.codex_home).map_err(BridgeError::Login)?;

    let runtime = Runtime::new().map_err(BridgeError::Runtime)?;

    if config.clear_existing {
        let result = runtime.block_on(logout_with_revoke(
            &config.codex_home,
            config.credentials_store_mode,
            config.keyring_backend_kind,
            &crate::default_auth_route_config(),
        ));
        if let Err(error) = result {
            // Match Codex CLI behavior: failure to revoke old credentials must not prevent login.
            eprintln!("codex-bridge: failed to clear existing auth before login: {error}");
        }
    }

    let mut options = ServerOptions::new(
        config.codex_home.clone(),
        CLIENT_ID.to_string(),
        config.forced_chatgpt_workspace_id.clone(),
        config.credentials_store_mode,
        config.keyring_backend_kind,
        crate::default_auth_route_config(),
    );
    options.open_browser = config.open_browser;
    options.port = config.port;
    if let Some(issuer) = &config.issuer {
        options.issuer = issuer.clone();
    }

    // run_login_server spawns its callback task, so enter the runtime while creating it.
    let _guard = runtime.enter();
    let server = run_login_server(options).map_err(BridgeError::Login)?;
    drop(_guard);

    let start_info = LoginStartInfo {
        auth_url: server.auth_url.clone(),
        callback_port: server.actual_port,
        codex_home: config.codex_home.display().to_string(),
    };
    let cancel_handle = server.cancel_handle();

    Ok(LoginSession {
        config,
        runtime,
        server: Mutex::new(Some(server)),
        cancel_handle,
        start_info,
    })
}

pub fn login_blocking(config: LoginConfig) -> Result<CredentialExport, BridgeError> {
    let session = start_login(config)?;
    session.wait()
}

pub fn export_credentials(config: &LoginConfig) -> Result<CredentialExport, BridgeError> {
    let auth = load_auth_dot_json(
        &config.codex_home,
        config.credentials_store_mode,
        config.keyring_backend_kind,
    )
    .map_err(BridgeError::Login)?
    .ok_or(BridgeError::NoCredentials)?;

    Ok(CredentialExport {
        codex_home: config.codex_home.display().to_string(),
        auth,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_start_and_cancel_real_codex_login_server() {
        let temp = tempfile::tempdir().expect("temp dir");
        let config = LoginConfig {
            codex_home: temp.path().to_path_buf(),
            open_browser: false,
            port: 0,
            clear_existing: false,
            ..LoginConfig::default()
        };

        let session = start_login(config).expect("start login server");
        assert!(
            session
                .start_info()
                .auth_url
                .starts_with("https://auth.openai.com/")
        );
        assert_ne!(session.start_info().callback_port, 0);
        session.cancel();
        let error = session.wait().expect_err("cancelled login must fail");
        assert!(error.to_string().contains("Login was not completed"));
    }
}
