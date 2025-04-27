use crate::executor::OutputError;
use semver::Version;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ComposerError {
    #[error("Neither 'docker compose' nor 'docker-compose' could be found or executed.")]
    CommandNotFound,

    #[error("Failed to execute docker compose command: {0}")]
    CommandExecutionError(String),

    #[error("Required environment variable(s) missing: {0:?}")]
    MissingEnvVars(Vec<String>),

    #[error("Failed to parse docker compose output: {0}")]
    ParseError(String),

    #[error("Invalid docker compose file at {path}: {details}")]
    InvalidComposeFile { path: PathBuf, details: String },

    #[error("Environment file error: {0}")]
    EnvFileError(String),

    #[error("YAML parsing error: {0}")]
    YamlError(#[from] serde_yaml::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Invalid configuration: {0}")]
    ConfigurationError(String),

    #[error("Docker compose version error: {0}")]
    VersionError(String),

    #[error(
        "Detected {command} version {version} is below the minimum required version {required}."
    )]
    VersionTooLow {
        command: String,
        version: Version,
        required: String,
    },

    #[error("Internal error: {0}")]
    InternalError(String),
}

// Implementation of utility methods for ComposerError
impl ComposerError {
    pub fn missing_vars(vars: Vec<String>) -> Self {
        ComposerError::MissingEnvVars(vars)
    }

    pub fn command_error(msg: impl Into<String>) -> Self {
        ComposerError::CommandExecutionError(msg.into())
    }

    pub fn parse_error(msg: impl Into<String>) -> Self {
        ComposerError::ParseError(msg.into())
    }

    pub fn invalid_compose_file(path: impl Into<PathBuf>, details: impl Into<String>) -> Self {
        ComposerError::InvalidComposeFile {
            path: path.into(),
            details: details.into(),
        }
    }
}

impl From<OutputError> for ComposerError {
    fn from(err: OutputError) -> Self {
        ComposerError::ParseError(err.to_string())
    }
}

// Type alias for Result with ComposerError
pub type ComposerResult<T> = Result<T, ComposerError>;
