use std::time::Duration;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no credentials for {provider}")]
    NoCredentials { provider: String },
    #[error("unknown model {0}")]
    UnknownModel(String),
    #[error("configuration file has unsafe permissions: {path}")]
    UnsafePermissions { path: String },
    #[error("provider requested retry delay {requested:?}, exceeding cap {cap:?}")]
    RetryDelayExceeded { requested: Duration, cap: Duration },
    #[error("context window exceeded: {0}")]
    ContextOverflow(String),
    #[error("request aborted")]
    Aborted,
    #[error("provider error: {0}")]
    Provider(String),
    #[error("cli error: {0}")]
    Cli(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    TomlDe(#[from] toml::de::Error),
    #[error(transparent)]
    TomlSer(#[from] toml::ser::Error),
}
