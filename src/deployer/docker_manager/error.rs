use crate::executor::{ExecutorError, OutputError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DockerError {
    #[error("Docker is not installed")]
    DockerNotInstalled,

    // Converts `ExecutorError` -> `DockerError::Executor(err)`
    #[error("Executor error: {0}")]
    Executor(#[from] ExecutorError),

    // Converts `OutputError` -> `DockerError::Output(err)`
    #[error("Output error: {0}")]
    Output(#[from] OutputError),

    #[error("Docker Compose is not installed")]
    DockerComposeNotInstalled,

    #[error("Unsupported operating system: {0}")]
    UnsupportedOS(String),

    #[error("Installation failed: {0}")]
    InstallationError(String),

    #[error("Working directory error: {0}")]
    WorkingDirError(String),

    #[error("Docker compose file not found: {0}")]
    ComposeFileNotFound(String),

    #[error("Command failed: {cmd} - {message}")]
    CommandError { cmd: String, message: String },

    #[error("Service health check failed: {0}")]
    HealthCheckError(String),

    #[error("Upload error: {0}")]
    UploadError(String),

    #[error("Docker compose error: {0}")]
    ComposeError(String),
}
