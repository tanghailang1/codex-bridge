//! Embeddable HTTP Responses bridge using Codex's own auth, HTTP transport, retries and SSE parser.

mod config;
mod engine;
mod error;
mod event;
pub mod ffi;
mod request;

pub use config::HttpBridgeConfig;
pub use engine::HttpBridge;
pub use engine::HttpRequestHandle;
pub use engine::NextEvent;
pub use error::HttpBridgeError;
pub use request::HttpRequest;

pub const HTTP_ABI_VERSION: u32 = 1;
pub const CODEX_COMMIT: &str = env!("CODEX_HTTP_BRIDGE_CODEX_COMMIT");
