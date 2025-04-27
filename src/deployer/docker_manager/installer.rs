use super::error::DockerError;
use super::types::{DockerResult, LinuxDistro};
use crate::deployer::types::ComposeExec;

pub struct DockerInstaller<'a> {
    executor: &'a mut (dyn ComposeExec + Send),
}

impl<'a> DockerInstaller<'a> {
    pub fn new(executor: &'a mut (dyn ComposeExec + Send)) -> Self {
        Self { executor }
    }

    pub async fn install_docker(&mut self, distro: &LinuxDistro) -> DockerResult<()> {
        match distro {
            LinuxDistro::Debian | LinuxDistro::Ubuntu => {
                let commands = [
                    "apt-get update",
                    "apt-get install -y ca-certificates curl gnupg",
                    "install -m 0755 -d /etc/apt/keyrings",
                    "curl -fsSL https://download.docker.com/linux/debian/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg",
                    "chmod a+r /etc/apt/keyrings/docker.gpg",
                    "echo \"deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian $(lsb_release -cs) stable\" | tee /etc/apt/sources.list.d/docker.list > /dev/null",
                    "apt-get update",
                    "apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin"
                ];

                for cmd in commands {
                    let result = self
                        .executor
                        .execute_command(cmd)
                        .await
                        .map_err(DockerError::from)?;

                    if !result.is_success() {
                        return Err(DockerError::InstallationError(format!(
                            "Failed to execute: {}",
                            cmd
                        )));
                    }
                }
                Ok(())
            }
            LinuxDistro::Unknown(os) => Err(DockerError::UnsupportedOS(os.clone())),
        }
    }

    pub async fn install_docker_compose(&mut self) -> DockerResult<()> {
        let commands = [
            "curl -L \"https://github.com/docker/compose/releases/download/v2.32.1/docker-compose-$(uname -s)-$(uname -m)\" -o /usr/local/bin/docker-compose",
            "chmod +x /usr/local/bin/docker-compose"
        ];

        for cmd in commands {
            let result = self
                .executor
                .execute_command(cmd)
                .await
                .map_err(DockerError::from)?;

            if !result.is_success() {
                return Err(DockerError::InstallationError(format!(
                    "Failed to execute: {}",
                    cmd
                )));
            }
        }
        Ok(())
    }
}
