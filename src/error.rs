use thiserror::Error;

#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("invalid login configuration: {0}")]
    InvalidConfig(#[from] serde_json::Error),
    #[error("failed to create async runtime: {0}")]
    Runtime(std::io::Error),
    #[error("ChatGPT login failed: {0}")]
    Login(std::io::Error),
    #[error("login session has already been awaited")]
    AlreadyWaited,
    #[error("no ChatGPT credentials are stored")]
    NoCredentials,
    #[error("invalid FFI argument: {0}")]
    InvalidArgument(&'static str),
}
