use super::docker_manager::DockerError;
use crate::composer::types::{PortMapping, VolumeMapping};
use crate::executor::{CommandExecutor, FileTransfer, OutputError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

pub trait ComposeExec: CommandExecutor + FileTransfer {}
impl<T: CommandExecutor + FileTransfer> ComposeExec for T {}

#[derive(Debug, Clone)]
pub struct DeploymentConfig {
    /// Local project directory
    pub project_dir: PathBuf,
    /// Remote directory where project will be deployed
    pub remote_dir: Option<PathBuf>,
    /// List of docker-compose files
    pub compose_files: Vec<PathBuf>,
    /// List of environment files
    pub env_files: Vec<PathBuf>,
    /// Environment variables required by the project
    pub consumed_env: HashMap<String, String>,
    /// Ports that need to be exposed
    pub exposed_ports: Vec<PortMapping>,
    /// Local files/directories that need to be synchronized
    pub local_references: Vec<PathBuf>,
    /// Volume mappings from compose file
    pub volumes: Vec<VolumeMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentStatus {
    /// Whether any project files have changed
    pub files_changed: bool,
    /// Whether environment configuration has changed
    pub env_changed: bool,
    /// Whether port configuration has changed
    pub ports_changed: bool,
    /// Whether all services are healthy
    pub services_healthy: bool,
    /// Detailed status message
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum DeployError {
    #[error("Docker manager error: {0}")]
    DockerManager(#[from] DockerError),

    #[error("File synchronization error: {0}")]
    FileSync(String),

    #[error("Environment error: {0}")]
    Environment(String),

    #[error("Firewall configuration error: {0}")]
    Firewall(String),

    #[error("Invalid configuration: {0}")]
    Configuration(String),

    #[error("Deployment failed: {0}")]
    Deployment(String),

    #[error("Output processing error: {0}")]
    OutputError(#[from] OutputError),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type DeployResult<T> = Result<T, DeployError>;

impl Default for DeploymentStatus {
    fn default() -> Self {
        Self::new()
    }
}

impl DeploymentStatus {
    pub fn new() -> Self {
        Self {
            files_changed: false,
            env_changed: false,
            ports_changed: false,
            services_healthy: false,
            message: String::new(),
        }
    }

    pub fn with_message(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            ..Self::new()
        }
    }

    pub fn is_successful(&self) -> bool {
        self.services_healthy && !self.has_pending_changes()
    }

    pub fn has_pending_changes(&self) -> bool {
        self.files_changed || self.env_changed || self.ports_changed
    }
}

#[derive(Debug, Clone)]
pub enum DeployerEvent {
    StepStarted(String),
    StepCompleted(String),
    StepFailed(String, String),
    HealthCheckAttempt(u32, u32),
    HealthCheckStatus(String),
}

impl fmt::Display for DeployerEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeployerEvent::StepStarted(msg) => write!(f, "Started: {}", msg),
            DeployerEvent::StepCompleted(msg) => write!(f, "Completed: {}", msg),
            DeployerEvent::StepFailed(step, err) => write!(f, "Failed: {} - {}", step, err),
            DeployerEvent::HealthCheckAttempt(a, t) => write!(f, "Health Check ({}/{})", a, t),
            DeployerEvent::HealthCheckStatus(s) => write!(f, "Health Status: {}", s),
        }
    }
}
