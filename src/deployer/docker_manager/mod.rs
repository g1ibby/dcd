mod error;
mod installer;
mod types;
mod validator;

use crate::deployer::types::ComposeExec;
use crate::executor::{CommandExecutor, CommandResult, FileTransfer, OutputError};
use async_trait::async_trait;
pub use error::DockerError;
use installer::DockerInstaller;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use types::{DockerResult, DockerVersion, LinuxDistro};
use validator::DockerValidator;

// --- New Types for Health Check ---

/// Represents details of a service that is not healthy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)] // Added Serialize/Deserialize for potential future use
pub struct UnhealthyService {
    pub name: String,
    pub state: String,  // e.g., "running", "exited"
    pub health: String, // e.g., "unhealthy", "starting", "" (if no healthcheck but not running)
    pub exit_code: i32,
    pub status: String, // Full status string like "Exited (1)" or "Up (unhealthy)"
}

/// Represents the overall health status of the deployment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthCheckResult {
    /// All services are running and healthy (or have no health check configured).
    Healthy,
    /// Contains services that are running but still in the 'starting' health state.
    /// All other services are Healthy.
    Starting(Vec<UnhealthyService>),
    /// Contains services that are definitively unhealthy (e.g., exited, health='unhealthy', dead).
    /// May also contain 'starting' services, but the presence of one terminal failure triggers this.
    Failed(Vec<UnhealthyService>),
    /// No services were found (e.g., compose file defines no services or `ps` returned empty).
    NoServices,
}

// --- End of New Types ---

#[async_trait]
pub trait DockerManager: Send {
    /// Check if Docker is installed and install if not
    async fn ensure_docker_installed(&mut self) -> DockerResult<()>;

    /// Check if Docker Compose is installed and install if not
    async fn ensure_docker_compose_installed(&mut self) -> DockerResult<()>;

    /// Get Docker version information
    async fn get_docker_version(&mut self) -> DockerResult<DockerVersion>;

    /// Verify docker-compose.yml exists in working directory
    async fn verify_compose_file(&mut self) -> DockerResult<()>;

    /// Get status of all services
    async fn get_services_status(&mut self) -> DockerResult<ComposeStatus>;

    /// Start services using docker-compose up -d
    async fn compose_up(&mut self) -> DockerResult<()>;

    /// Upload docker-compose.yml file
    async fn upload_compose_file(
        &mut self,
        local_path: &Path, // Changed from generic P to &Path
    ) -> DockerResult<()>;

    /// Check the health status of all services.
    /// Returns detailed information about unhealthy services if any.
    async fn verify_services_healthy(&mut self) -> DockerResult<HealthCheckResult>;

    /// Check if there are any running services
    async fn has_running_services(&mut self) -> DockerResult<bool>;

    /// Stop services using docker-compose down with optional volume and image removal
    async fn compose_down(&mut self, remove_volumes: bool, remove_images: bool)
        -> DockerResult<()>;

    /// Remove a specific volume
    async fn remove_volume(&mut self, volume_name: &str) -> DockerResult<()>;

    /// Remove unused images to save disk space
    async fn prune_images(&mut self) -> DockerResult<()>;
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Publisher {
    #[serde(rename = "URL")]
    pub url: String,
    #[serde(rename = "TargetPort")]
    pub target_port: u16,
    #[serde(rename = "PublishedPort")]
    pub published_port: u16,
    #[serde(rename = "Protocol")]
    pub protocol: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServiceStatus {
    #[serde(rename = "Command")]
    pub command: String,
    #[serde(rename = "CreatedAt")]
    pub created_at: String,
    #[serde(rename = "ExitCode")]
    pub exit_code: i32,
    #[serde(rename = "Health")]
    pub health: String,
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Image")]
    pub image: String,
    #[serde(rename = "Labels")]
    pub labels: String, // Could be converted to HashMap if needed
    #[serde(rename = "LocalVolumes")]
    pub local_volumes: String,
    #[serde(rename = "Mounts")]
    pub mounts: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Names")]
    pub names: String,
    #[serde(rename = "Networks")]
    pub networks: String,
    #[serde(rename = "Ports")]
    pub ports: String,
    #[serde(rename = "Project")]
    pub project: String,
    #[serde(rename = "Publishers")]
    pub publishers: Vec<Publisher>,
    #[serde(rename = "RunningFor")]
    pub running_for: String,
    #[serde(rename = "Service")]
    pub service: String,
    #[serde(rename = "Size")]
    pub size: String,
    #[serde(rename = "State")]
    pub state: String,
    #[serde(rename = "Status")]
    pub status: String,
}

pub trait DockerExec: CommandExecutor + FileTransfer {}

impl<T: CommandExecutor + FileTransfer> DockerExec for T {}

#[derive(Debug, Serialize, Deserialize)]
pub struct ComposeStatus {
    #[serde(flatten)]
    pub services: Vec<ServiceStatus>,
}

impl ServiceStatus {
    pub fn is_running(&self) -> bool {
        self.state == "running"
    }

    /// A service is considered healthy if it's running AND
    /// (it has no health check OR its health check reports "healthy").
    pub fn is_healthy(&self) -> bool {
        self.is_running() && (self.health.is_empty() || self.health == "healthy")
    }

    pub fn get_ports(&self) -> Vec<(u16, u16)> {
        self.publishers
            .iter()
            .map(|p| (p.published_port, p.target_port))
            .collect()
    }
}

impl ComposeStatus {
    pub fn new(services: Vec<ServiceStatus>) -> Self {
        Self { services }
    }

    pub fn all_running(&self) -> bool {
        self.services.iter().all(|s| s.is_running())
    }

    pub fn all_healthy(&self) -> bool {
        self.services.iter().all(|s| s.is_healthy())
    }

    pub fn get_service(&self, name: &str) -> Option<&ServiceStatus> {
        self.services.iter().find(|s| s.service == name)
    }
}

pub struct SshDockerManager<'a> {
    executor: &'a mut (dyn ComposeExec + Send),
    distro: LinuxDistro,
    working_directory: PathBuf,
    /// List of compose files (relative to working directory or absolute paths)
    compose_files: Vec<PathBuf>,
    /// List of env files
    env_files: Vec<PathBuf>,
}

impl<'a> SshDockerManager<'a> {
    /// Create a new Docker manager for SSH, specifying compose and env files to use.
    pub async fn new(
        executor: &'a mut (dyn ComposeExec + Send),
        working_directory: PathBuf,
        compose_files: Vec<PathBuf>,
        env_files: Vec<PathBuf>,
    ) -> DockerResult<Self> {
        let mut validator = DockerValidator::new(executor);
        let distro = validator.detect_distro().await?;

        let mut manager = Self {
            executor,
            distro,
            working_directory,
            compose_files,
            env_files,
        };

        // Verify working directory exists
        manager.verify_working_directory().await?;

        Ok(manager)
    }

    async fn verify_working_directory(&mut self) -> DockerResult<()> {
        let cmd = format!(
            "test -d {} && echo 'exists'",
            self.working_directory.display()
        );
        let result = self.executor.execute_command(&cmd).await?;
        if !result.is_success() {
            return Err(DockerError::WorkingDirError(
                "Working directory does not exist".into(),
            ));
        }
        Ok(())
    }

    async fn execute_compose_command(&mut self, cmd: &str) -> DockerResult<CommandResult> {
        let full_cmd = format!("cd {} && {}", self.working_directory.display(), cmd);
        self.executor
            .execute_command(&full_cmd)
            .await
            .map_err(DockerError::from)
    }

    /// Build a docker-compose command string with configured compose files and env files.
    fn format_docker_compose_command(&self, subcommand: &str) -> String {
        let mut cmd = String::from("docker-compose");
        for cf in &self.compose_files {
            cmd.push_str(" -f ");
            cmd.push_str(&cf.to_string_lossy());
        }
        for ef in &self.env_files {
            cmd.push_str(" --env-file ");
            cmd.push_str(&ef.to_string_lossy());
        }
        cmd.push(' ');
        cmd.push_str(subcommand);
        cmd
    }
}

#[async_trait]
impl DockerManager for SshDockerManager<'_> {
    #[inline]
    async fn ensure_docker_installed(&mut self) -> DockerResult<()> {
        let mut validator = DockerValidator::new(self.executor);
        if !validator.is_docker_installed().await? {
            let mut installer = DockerInstaller::new(self.executor);
            installer.install_docker(&self.distro).await?;
        }
        Ok(())
    }

    #[inline]
    async fn ensure_docker_compose_installed(&mut self) -> DockerResult<()> {
        let mut validator = DockerValidator::new(self.executor);
        if !validator.is_docker_compose_installed().await? {
            let mut installer = DockerInstaller::new(self.executor);
            installer.install_docker_compose().await?;
        }
        Ok(())
    }

    #[inline]
    async fn get_docker_version(&mut self) -> DockerResult<DockerVersion> {
        let mut validator = DockerValidator::new(self.executor);
        validator.get_docker_version().await
    }

    async fn get_services_status(&mut self) -> DockerResult<ComposeStatus> {
        // Use configured compose and env files when listing services
        let cmd = self.format_docker_compose_command("ps --format json");
        let result = self.execute_compose_command(&cmd).await?;

        if !result.is_success() {
            return Err(DockerError::CommandError {
                cmd: "docker-compose ps".to_string(),
                message: result.output.to_stderr_string()?,
            });
        }

        // Parse the JSON output into ServiceStatus structs.
        // Some versions emit a JSON array, others emit one JSON object per line.
        let stdout = result.output.to_stdout_string()?;
        let services: Vec<ServiceStatus> = if stdout.trim_start().starts_with('[') {
            // entire array as JSON
            serde_json::from_str(&stdout)
                .map_err(|e| DockerError::Output(OutputError::JsonError(e)))?
        } else {
            // line-delimited JSON objects
            let mut v = Vec::new();
            for line in stdout.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let svc: ServiceStatus = serde_json::from_str(line)
                    .map_err(|e| DockerError::Output(OutputError::JsonError(e)))?;
                v.push(svc);
            }
            v
        };
        Ok(ComposeStatus { services })
    }

    async fn compose_up(&mut self) -> DockerResult<()> {
        // First prune unused images to save disk space
        self.prune_images().await?;

        // Pull latest images and start services with configured compose and env files
        let pull_cmd = self.format_docker_compose_command("pull");
        let up_cmd = self.format_docker_compose_command("up -d --remove-orphans");
        let commands = [pull_cmd, up_cmd];

        for cmd in &commands {
            tracing::info!("Executing compose command: '{}'", cmd);
            let result = self.execute_compose_command(cmd).await?;

            if !result.is_success() {
                let error_msg = result.output.to_stderr_string()?;
                tracing::error!("Compose command failed: '{}'. Error: {}", cmd, error_msg);
                return Err(DockerError::CommandError {
                    cmd: cmd.to_string(),
                    message: error_msg,
                });
            }
        }
        Ok(())
    }

    async fn verify_services_healthy(&mut self) -> DockerResult<HealthCheckResult> {
        let status = self.get_services_status().await?;

        if status.services.is_empty() {
            tracing::warn!("No services found when checking health status.");
            return Ok(HealthCheckResult::NoServices);
        }

        let mut starting_services = Vec::new();
        let mut failed_services = Vec::new();

        for s in status.services.iter() {
            // Base definition of "healthy" - running and (no healthcheck or health='healthy')
            let is_technically_healthy =
                s.is_running() && (s.health.is_empty() || s.health == "healthy");

            if !is_technically_healthy {
                let unhealthy_detail = UnhealthyService {
                    name: s.service.clone(),
                    state: s.state.clone(),
                    health: s.health.clone(),
                    exit_code: s.exit_code,
                    status: s.status.clone(),
                };

                // Categorize: Is it just starting or actually failed?
                if s.is_running() && s.health == "starting" {
                    // It's running but health is 'starting' -> Potential recovery
                    starting_services.push(unhealthy_detail);
                } else {
                    // It's exited, restarting, dead, or health='unhealthy' -> Definitive failure
                    failed_services.push(unhealthy_detail);
                }
            }
        }

        if !failed_services.is_empty() {
            // If any service has definitively failed, report Failed overall.
            // Include starting services in the report for completeness.
            tracing::warn!(
                "Found definitively failed services: {:?}",
                failed_services.iter().map(|s| &s.name).collect::<Vec<_>>()
            );
            failed_services.extend(starting_services); // Combine lists
            Ok(HealthCheckResult::Failed(failed_services))
        } else if !starting_services.is_empty() {
            // No failed services, but some are still starting.
            tracing::info!(
                "Found services still starting: {:?}",
                starting_services
                    .iter()
                    .map(|s| &s.name)
                    .collect::<Vec<_>>()
            );
            Ok(HealthCheckResult::Starting(starting_services))
        } else {
            // All services are technically healthy.
            Ok(HealthCheckResult::Healthy)
        }
    }

    async fn verify_compose_file(&mut self) -> DockerResult<()> {
        let compose_path = self.working_directory.join("docker-compose.yml");
        let cmd = format!("test -f {}", compose_path.display());

        let result = self
            .executor
            .execute_command(&cmd)
            .await
            .map_err(DockerError::from)?;

        if !result.is_success() {
            return Err(DockerError::ComposeError(
                "docker-compose.yml not found".to_string(),
            ));
        }
        Ok(())
    }

    async fn upload_compose_file(&mut self, local_path: &Path) -> DockerResult<()> {
        let remote_path = self.working_directory.join("docker-compose.yml");
        self.executor
            .upload_file(local_path, remote_path.as_ref())
            .await
            .map_err(DockerError::from)?;
        self.verify_compose_file().await
    }

    async fn has_running_services(&mut self) -> DockerResult<bool> {
        let status = self.get_services_status().await?;
        Ok(status.services.iter().any(|s| s.is_running()))
    }

    async fn compose_down(
        &mut self,
        remove_volumes: bool,
        remove_images: bool,
    ) -> DockerResult<()> {
        // Stop services and remove containers/networks with configured compose and env files
        let mut cmd = self.format_docker_compose_command("down");
        if remove_volumes {
            cmd.push_str(" -v");
        }
        if remove_images {
            cmd.push_str(" --rmi all");
        }
        let result = self.execute_compose_command(&cmd).await?;
        if !result.is_success() {
            return Err(DockerError::CommandError {
                cmd: cmd.to_string(),
                message: result
                    .output
                    .to_stderr_string()
                    .map_err(DockerError::from)?,
            });
        }
        Ok(())
    }

    async fn remove_volume(&mut self, volume_name: &str) -> DockerResult<()> {
        let cmd = format!("docker volume rm {}", volume_name);
        let result = self.executor.execute_command(&cmd).await?;

        // If the volume doesn't exist, that's fine
        if !result.is_success() && !result.output.to_stderr_string()?.contains("No such volume") {
            return Err(DockerError::CommandError {
                cmd,
                message: result.output.to_stderr_string()?,
            });
        }
        Ok(())
    }

    async fn prune_images(&mut self) -> DockerResult<()> {
        tracing::info!("Pruning unused images for current project to save disk space...");

        // Get the project name from the working directory (used by docker-compose for labeling)
        let project_name = self
            .working_directory
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "default".to_string());

        // First, try to get current project images to understand what we're working with
        let images_cmd = self.format_docker_compose_command("images --format json");
        if let Ok(images_result) = self.execute_compose_command(&images_cmd).await {
            if images_result.is_success() {
                let stdout = images_result.output.to_stdout_string().unwrap_or_default();
                if !stdout.trim().is_empty() {
                    tracing::debug!("Current project images: {}", stdout.trim());
                }
            }
        }

        // Prune images with compose project label to target only this project's unused images
        let cmd = format!(
            "docker image prune -f --filter label=com.docker.compose.project={}",
            project_name
        );

        let result = self.executor.execute_command(&cmd).await?;

        if !result.is_success() {
            let error_msg = result.output.to_stderr_string()?;
            tracing::warn!("Project image pruning failed: {}", error_msg);
            return Err(DockerError::CommandError {
                cmd: cmd.to_string(),
                message: error_msg,
            });
        }

        let stdout = result.output.to_stdout_string()?;
        if !stdout.trim().is_empty() {
            tracing::info!("Project image pruning result: {}", stdout.trim());
        } else {
            tracing::info!("No unused project images found to prune.");
        }
        Ok(())
    }
}
