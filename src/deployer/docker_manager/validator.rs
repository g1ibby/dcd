use super::error::DockerError;
use super::types::{DockerResult, DockerVersion, LinuxDistro};
use crate::deployer::types::ComposeExec;

pub struct DockerValidator<'a> {
    executor: &'a mut (dyn ComposeExec + Send),
}

impl<'a> DockerValidator<'a> {
    pub fn new(executor: &'a mut (dyn ComposeExec + Send)) -> Self {
        Self { executor }
    }

    pub async fn detect_distro(&mut self) -> DockerResult<LinuxDistro> {
        let result = self
            .executor
            .execute_command("cat /etc/os-release")
            .await
            .map_err(DockerError::from)?;

        if !result.is_success() {
            return Ok(LinuxDistro::Unknown("Failed to detect".to_string()));
        }

        let output = result
            .output
            .to_stdout_string()
            .map_err(DockerError::from)?;

        if output.contains("debian") {
            Ok(LinuxDistro::Debian)
        } else if output.contains("ubuntu") {
            Ok(LinuxDistro::Ubuntu)
        } else {
            Ok(LinuxDistro::Unknown(output))
        }
    }

    pub async fn is_docker_installed(&mut self) -> DockerResult<bool> {
        let result = self
            .executor
            .execute_command("docker --version")
            .await
            .map_err(DockerError::from)?;

        Ok(result.is_success())
    }

    pub async fn is_docker_compose_installed(&mut self) -> DockerResult<bool> {
        let result = self
            .executor
            .execute_command("docker-compose --version")
            .await
            .map_err(DockerError::from)?;

        Ok(result.is_success())
    }

    pub async fn get_docker_version(&mut self) -> DockerResult<DockerVersion> {
        let result = self
            .executor
            .execute_command("docker version --format '{{.Server.Version}}'")
            .await
            .map_err(DockerError::from)?;

        if !result.is_success() {
            return Err(DockerError::DockerNotInstalled);
        }

        let version = result
            .output
            .to_stdout_string()
            .map_err(DockerError::from)?
            .trim()
            .to_string();

        Ok(DockerVersion { version })
    }
}
