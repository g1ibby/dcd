use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("Operation failed: {0}")]
    OperationFailed(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}
