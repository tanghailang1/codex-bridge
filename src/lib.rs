//! Embeddable bridge around selected Codex CLI capabilities.
//!
//! The Rust API is convenient for tests and native Rust callers. The exported C ABI in [`ffi`]
//! is the stable integration boundary for proxy processes written in other languages.

mod config;
mod error;
mod login;
mod models;
mod refresh;
mod status;
mod template;

pub mod ffi;

pub use config::LoginConfig;
pub use error::BridgeError;
pub use login::CredentialExport;
pub use login::LoginSession;
pub use login::LoginStartInfo;
pub use login::export_credentials;
pub use login::login_blocking;
pub use login::start_login;
pub use models::list_models;
pub use refresh::refresh_credentials;
pub use status::query_account_status;
pub use template::responses_request_schema;

/// ABI version of the exported C interface.
pub const ABI_VERSION: u32 = 1;

/// Exact Codex submodule commit compiled into this bridge.
pub const CODEX_COMMIT: &str = env!("CODEX_BRIDGE_CODEX_COMMIT");
