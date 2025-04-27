use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum ExecutorError {
    #[error("SSH error: {0}")]
    SshError(String),

    #[error("Local command error: {0}")]
    LocalError(String),

    #[error("Generic executor error: {0}")]
    Other(String),
}
