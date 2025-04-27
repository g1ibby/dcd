use super::validator::VariablesValidator;
use crate::composer::types::{ComposerResult, ComposerVariables};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct EnvironmentStatus {
    pub available_in_system: Vec<String>,
    pub available_in_env_file: Vec<String>,
    pub available_from_defaults: Vec<String>,
    pub missing_required: Vec<String>,
    pub missing_optional: Vec<String>,
}

impl Default for EnvironmentStatus {
    fn default() -> Self {
        Self::new()
    }
}

impl EnvironmentStatus {
    pub fn new() -> Self {
        Self {
            available_in_system: Vec::new(),
            available_in_env_file: Vec::new(),
            available_from_defaults: Vec::new(),
            missing_required: Vec::new(),
            missing_optional: Vec::new(),
        }
    }

    pub fn get_resolved_variables(&self) -> HashMap<String, String> {
        let mut resolved = HashMap::new();

        // Add system environment variables
        for var_name in &self.available_in_system {
            if let Ok(value) = std::env::var(var_name) {
                resolved.insert(var_name.clone(), value);
            }
        }

        // Add .env file variables
        for var_name in &self.available_in_env_file {
            if let Ok(value) = std::env::var(var_name) {
                resolved.insert(var_name.clone(), value);
            }
        }

        // Add default values
        for var_name in &self.available_from_defaults {
            if let Ok(value) = std::env::var(var_name) {
                resolved.insert(var_name.clone(), value);
            }
        }

        resolved
    }

    pub fn is_valid(&self) -> bool {
        self.missing_required.is_empty()
    }
}

pub struct EnvironmentChecker {
    validator: VariablesValidator,
}

impl Default for EnvironmentChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl EnvironmentChecker {
    pub fn new() -> Self {
        Self {
            validator: VariablesValidator::new(),
        }
    }

    /// Check availability of all required variables
    pub async fn check_environment(
        &mut self,
        variables: &[ComposerVariables],
        env_files: &[PathBuf],
    ) -> ComposerResult<EnvironmentStatus> {
        self.validator.load_env_files(env_files)?;
        let mut status = EnvironmentStatus::new();

        for var in variables {
            self.check_variable_availability(var, &mut status)?;
        }

        Ok(status)
    }

    fn check_variable_availability(
        &self,
        var: &ComposerVariables,
        status: &mut EnvironmentStatus,
    ) -> ComposerResult<()> {
        // Check system environment
        if std::env::var(&var.name).is_ok() {
            status.available_in_system.push(var.name.clone());
            return Ok(());
        }

        // Check .env file
        if self.validator.has_env_file_variable(&var.name) {
            status.available_in_env_file.push(var.name.clone());
            return Ok(());
        }

        // Check defaults
        if var.default_value.is_some() {
            status.available_from_defaults.push(var.name.clone());
            return Ok(());
        }

        // Variable is missing
        if var.required {
            status.missing_required.push(var.name.clone());
        } else {
            status.missing_optional.push(var.name.clone());
        }

        Ok(())
    }

    /// Get all available environment variables with their values
    pub fn get_available_variables(&self) -> HashMap<String, String> {
        let mut available = HashMap::new();

        // System environment takes precedence
        for (key, value) in std::env::vars() {
            available.insert(key, value);
        }

        // Add .env file variables (don't override system env)
        for (key, value) in self.validator.get_env_file_variables() {
            available.entry(key.clone()).or_insert(value.clone());
        }

        available
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_env_file(dir: &TempDir, content: &str) -> std::io::Result<()> {
        let env_path = dir.path().join(".env");
        let mut file = File::create(env_path)?;
        write!(file, "{}", content)?;
        Ok(())
    }

    #[tokio::test]
    async fn test_environment_checker() -> ComposerResult<()> {
        let temp_dir = TempDir::new().unwrap();
        create_test_env_file(&temp_dir, "ENV_FILE_VAR=value\nDB_PORT=5432").unwrap();

        std::env::set_var("SYSTEM_VAR", "system_value");

        let variables = vec![
            ComposerVariables {
                name: "SYSTEM_VAR".to_string(),
                required: true,
                default_value: None,
                alternate_value: None,
            },
            ComposerVariables {
                name: "ENV_FILE_VAR".to_string(),
                required: true,
                default_value: None,
                alternate_value: None,
            },
            ComposerVariables {
                name: "DEFAULT_VAR".to_string(),
                required: false,
                default_value: Some("default".to_string()),
                alternate_value: None,
            },
            ComposerVariables {
                name: "MISSING_REQUIRED".to_string(),
                required: true,
                default_value: None,
                alternate_value: None,
            },
            ComposerVariables {
                name: "MISSING_OPTIONAL".to_string(),
                required: false,
                default_value: None,
                alternate_value: None,
            },
        ];

        let mut checker = EnvironmentChecker::new();
        let status = checker
            .check_environment(&variables, &[temp_dir.path().join(".env")])
            .await?;

        assert!(!status.is_valid());
        assert_eq!(status.available_in_system, vec!["SYSTEM_VAR"]);
        assert_eq!(status.available_in_env_file, vec!["ENV_FILE_VAR"]);
        assert_eq!(status.available_from_defaults, vec!["DEFAULT_VAR"]);
        assert_eq!(status.missing_required, vec!["MISSING_REQUIRED"]);
        assert_eq!(status.missing_optional, vec!["MISSING_OPTIONAL"]);

        Ok(())
    }
}
