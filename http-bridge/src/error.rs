use thiserror::Error;

#[derive(Debug, Error)]
pub enum HttpBridgeError {
    #[error("invalid configuration or request JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("failed to create async runtime: {0}")]
    Runtime(std::io::Error),
    #[error("ChatGPT credentials are unavailable; complete bridge login first")]
    NoCredentials,
    #[error("the stored credential is not ChatGPT authentication")]
    NotChatGpt,
    #[error("Codex provider configuration failed: {0}")]
    Provider(String),
    #[error("Codex HTTP client construction failed: {0}")]
    HttpClient(String),
    #[error("Codex Responses request failed: {0}")]
    Request(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("invalid FFI argument: {0}")]
    InvalidArgument(&'static str),
    #[error("event receiver is already in use")]
    ReceiverBusy,
}
