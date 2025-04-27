use super::{PortConfig, Protocol};
use crate::deployer::types::{ComposeExec, DeployError, DeployResult};
use std::collections::HashSet;

pub struct UfwManager<'a> {
    executor: &'a mut (dyn ComposeExec + Send),
}

impl<'a> UfwManager<'a> {
    pub fn new(executor: &'a mut (dyn ComposeExec + Send)) -> Self {
        Self { executor }
    }

    /// Ensure UFW is installed and enabled
    pub async fn ensure_ufw(&mut self) -> DeployResult<()> {
        // Check if UFW is installed
        let result = self
            .executor
            .execute_command("which ufw")
            .await
            .map_err(|e| DeployError::Firewall(format!("Failed to check UFW: {}", e)))?;

        if !result.is_success() {
            // Install UFW
            let install_cmd = "apt-get update && apt-get install -y ufw";
            self.executor
                .execute_command(install_cmd)
                .await
                .map_err(|e| DeployError::Firewall(format!("Failed to install UFW: {}", e)))?;
        }

        // Enable UFW if not already enabled
        let status = self
            .executor
            .execute_command("ufw status")
            .await
            .map_err(|e| DeployError::Firewall(format!("Failed to check UFW status: {}", e)))?;

        if !status.output.to_stdout_string()?.contains("Status: active") {
            // Allow SSH first to prevent lockout
            self.executor
                .execute_command("ufw allow 22/tcp")
                .await
                .map_err(|e| DeployError::Firewall(format!("Failed to allow SSH: {}", e)))?;

            // Enable UFW
            self.executor
                .execute_command("echo 'y' | ufw enable")
                .await
                .map_err(|e| DeployError::Firewall(format!("Failed to enable UFW: {}", e)))?;
        }

        Ok(())
    }

    /// Configure ports in UFW
    pub async fn configure_ports(&mut self, ports: &[PortConfig]) -> DeployResult<()> {
        // Ensure UFW is ready
        self.ensure_ufw().await?;

        // Get currently opened ports
        let current_ports = self.get_opened_ports().await?;

        // Configure each port
        for port_config in ports {
            if !self.is_port_configured(&current_ports, port_config) {
                self.add_port_rule(port_config).await?;
            }
        }

        Ok(())
    }

    /// Get currently opened ports
    async fn get_opened_ports(&mut self) -> DeployResult<HashSet<String>> {
        let result = self
            .executor
            .execute_command("ufw status numbered")
            .await
            .map_err(|e| DeployError::Firewall(format!("Failed to get UFW status: {}", e)))?;

        let output = result.output.to_stdout_string()?;
        let mut ports = HashSet::new();

        for line in output.lines() {
            if line.contains("ALLOW") {
                if let Some(port_str) = self.extract_port_from_rule(line) {
                    ports.insert(port_str);
                }
            }
        }

        Ok(ports)
    }

    /// Extract port and protocol from UFW rule
    fn extract_port_from_rule(&self, rule: &str) -> Option<String> {
        // Example rule: "[ 2] 80/tcp                     ALLOW IN    Anywhere"
        let parts: Vec<&str> = rule.split_whitespace().collect();
        parts
            .iter()
            .find(|&&p| p.contains('/'))
            .map(|&s| s.to_string())
    }

    /// Check if port is already configured
    fn is_port_configured(&self, current_ports: &HashSet<String>, config: &PortConfig) -> bool {
        match config.protocol {
            Protocol::Both => {
                current_ports.contains(&format!("{}/tcp", config.port))
                    && current_ports.contains(&format!("{}/udp", config.port))
            }
            _ => current_ports.contains(&format!("{}/{}", config.port, config.protocol)),
        }
    }

    /// Add new port rule
    async fn add_port_rule(&mut self, config: &PortConfig) -> DeployResult<()> {
        let comment = if config.description.is_empty() {
            "Managed by DCD".to_string()
        } else {
            format!("DCD: {}", config.description)
        };

        match config.protocol {
            Protocol::Both => {
                self.add_single_port_rule(config.port, "tcp", &comment)
                    .await?;
                self.add_single_port_rule(config.port, "udp", &comment)
                    .await?;
            }
            _ => {
                self.add_single_port_rule(config.port, &config.protocol.to_string(), &comment)
                    .await?;
            }
        }

        Ok(())
    }

    /// Add single port rule with specific protocol
    async fn add_single_port_rule(
        &mut self,
        port: u16,
        protocol: &str,
        comment: &str,
    ) -> DeployResult<()> {
        let cmd = format!(
            "ufw allow {}/{} comment '{}'",
            port,
            protocol,
            comment.replace('\'', "\\'")
        );

        self.executor.execute_command(&cmd).await.map_err(|e| {
            DeployError::Firewall(format!(
                "Failed to add port rule {}/{}: {}",
                port, protocol, e
            ))
        })?;

        Ok(())
    }

    /// Verify port is accessible
    pub async fn verify_port(&mut self, port: u16, protocol: &Protocol) -> DeployResult<bool> {
        // For TCP, we can use nc to test
        if matches!(protocol, Protocol::Tcp | Protocol::Both) {
            let cmd = format!("nc -z -v localhost {}", port);
            let result = self
                .executor
                .execute_command(&cmd)
                .await
                .map_err(|e| DeployError::Firewall(e.to_string()))?;

            if !result.is_success() {
                return Ok(false);
            }
        }

        // For UDP, we can only verify the rule exists
        if matches!(protocol, Protocol::Udp | Protocol::Both) {
            let ports = self.get_opened_ports().await?;
            if !ports.contains(&format!("{}/udp", port)) {
                return Ok(false);
            }
        }

        Ok(true)
    }
}
