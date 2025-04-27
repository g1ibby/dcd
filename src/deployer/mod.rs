pub mod docker_manager;
pub mod firewall;
pub mod service;
pub mod sync;
pub mod types;
pub use service::Deployer;
use types::{DeployError, DeployResult, DeploymentConfig};

pub const DCD_ENV_FILE: &str = ".env.dcd";
pub const BACKUP_SUFFIX: &str = ".backup";

/// Deployment configuration validation
pub fn validate_config(config: &DeploymentConfig) -> DeployResult<()> {
    // Validate project directory
    if !config.project_dir.exists() {
        return Err(DeployError::Configuration(
            "Project directory does not exist".into(),
        ));
    }

    // Validate compose files
    for file in &config.compose_files {
        if !file.exists() {
            return Err(DeployError::Configuration(format!(
                "Compose file not found: {}",
                file.display()
            )));
        }
    }

    // Validate env files
    for file in &config.env_files {
        if !file.exists() {
            return Err(DeployError::Configuration(format!(
                "Environment file not found: {}",
                file.display()
            )));
        }
    }

    // Validate local references
    for path in &config.local_references {
        if !path.exists() {
            return Err(DeployError::Configuration(format!(
                "Referenced file/directory not found: {}",
                path.display()
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_config(temp_dir: &TempDir) -> DeploymentConfig {
        // Create test files
        fs::write(temp_dir.path().join("docker-compose.yml"), "version: '3'").unwrap();
        fs::write(temp_dir.path().join(".env"), "VAR=value").unwrap();
        fs::create_dir(temp_dir.path().join("config")).unwrap();

        DeploymentConfig {
            project_dir: temp_dir.path().to_path_buf(),
            remote_dir: Some(PathBuf::from("/remote/dir")),
            compose_files: vec![temp_dir.path().join("docker-compose.yml")],
            env_files: vec![temp_dir.path().join(".env")],
            consumed_env: HashMap::new(),
            exposed_ports: Vec::new(),
            local_references: vec![temp_dir.path().join("config")],
            volumes: Vec::new(),
        }
    }

    #[test]
    fn test_validate_config_valid() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);

        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_validate_config_missing_files() {
        let config = DeploymentConfig {
            project_dir: PathBuf::from("/nonexistent"),
            remote_dir: Some(PathBuf::from("/remote/dir")),
            compose_files: vec![PathBuf::from("/nonexistent/docker-compose.yml")],
            env_files: vec![],
            consumed_env: HashMap::new(),
            exposed_ports: Vec::new(),
            local_references: Vec::new(),
            volumes: Vec::new(),
        };

        assert!(validate_config(&config).is_err());
    }
}
