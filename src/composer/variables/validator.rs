use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use crate::composer::{
    errors::ComposerError,
    types::{ComposerResult, ComposerVariables},
};

pub struct VariablesValidator {
    system_env: HashMap<String, String>,
    env_file_vars: HashMap<String, String>,
}

impl Default for VariablesValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl VariablesValidator {
    pub fn new() -> Self {
        Self {
            system_env: std::env::vars().collect(),
            env_file_vars: HashMap::new(),
        }
    }

    pub fn load_env_files(&mut self, env_files: &[PathBuf]) -> ComposerResult<()> {
        for env_file in env_files {
            if !env_file.exists() {
                return Err(ComposerError::EnvFileError(format!(
                    "Env file not found: {}",
                    env_file.display()
                )));
            }

            let content = fs::read_to_string(env_file).map_err(|e| {
                ComposerError::EnvFileError(format!(
                    "Failed to read env file {}: {}",
                    env_file.display(),
                    e
                ))
            })?;

            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim();
                    let value = value.trim().trim_matches('"').trim_matches('\'');
                    self.env_file_vars
                        .insert(key.to_string(), value.to_string());
                }
            }
        }

        Ok(())
    }

    /// Load variables from .env file if it exists
    pub fn load_env_file(&mut self, project_dir: &Path) -> ComposerResult<()> {
        let env_path = project_dir.join(".env");
        if !env_path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&env_path)
            .map_err(|e| ComposerError::EnvFileError(format!("Failed to read .env file: {}", e)))?;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"').trim_matches('\'');
                self.env_file_vars
                    .insert(key.to_string(), value.to_string());
            }
        }

        Ok(())
    }

    /// Validate variables against current environment and .env file
    pub fn validate_variables(
        &self,
        variables: &[ComposerVariables],
    ) -> ComposerResult<ValidationResult> {
        let mut result = ValidationResult::new();

        for var in variables {
            let value = self.resolve_variable(var);

            match value {
                Some(val) => {
                    result.resolved_vars.insert(var.name.clone(), val);
                }
                None => {
                    if var.required {
                        result.missing_vars.push(var.name.clone());
                    } else if let Some(default) = &var.default_value {
                        result
                            .resolved_vars
                            .insert(var.name.clone(), default.clone());
                    }
                }
            }
        }

        Ok(result)
    }

    fn resolve_variable(&self, var: &ComposerVariables) -> Option<String> {
        // Check system environment first
        if let Some(value) = self.system_env.get(&var.name) {
            return Some(value.clone());
        }

        // Then check .env file
        if let Some(value) = self.env_file_vars.get(&var.name) {
            return Some(value.clone());
        }

        None
    }

    pub fn has_env_file_variable(&self, name: &str) -> bool {
        self.env_file_vars.contains_key(name)
    }

    pub fn get_env_file_variables(&self) -> &HashMap<String, String> {
        &self.env_file_vars
    }
}

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub resolved_vars: HashMap<String, String>,
    pub missing_vars: Vec<String>,
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self::new()
    }
}

impl ValidationResult {
    pub fn new() -> Self {
        Self {
            resolved_vars: HashMap::new(),
            missing_vars: Vec::new(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.missing_vars.is_empty()
    }

    pub fn get_resolved(&self, name: &str) -> Option<&String> {
        self.resolved_vars.get(name)
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

    #[test]
    fn test_load_env_file() -> ComposerResult<()> {
        let temp_dir = TempDir::new().unwrap();
        create_test_env_file(&temp_dir, "TEST_VAR=value\nDB_PORT=5432").unwrap();

        let mut validator = VariablesValidator::new();
        validator.load_env_file(temp_dir.path())?;

        assert_eq!(
            validator.env_file_vars.get("TEST_VAR"),
            Some(&"value".to_string())
        );
        assert_eq!(
            validator.env_file_vars.get("DB_PORT"),
            Some(&"5432".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_validate_variables() -> ComposerResult<()> {
        let temp_dir = TempDir::new().unwrap();
        create_test_env_file(&temp_dir, "PG_PASS=secret\n").unwrap();

        let mut validator = VariablesValidator::new();
        validator.load_env_file(temp_dir.path())?;

        let variables = vec![
            ComposerVariables {
                name: "PG_PASS".to_string(),
                required: true,
                default_value: None,
                alternate_value: None,
            },
            ComposerVariables {
                name: "DB_PORT".to_string(),
                required: false,
                default_value: Some("5432".to_string()),
                alternate_value: None,
            },
            ComposerVariables {
                name: "REQUIRED_VAR".to_string(),
                required: true,
                default_value: None,
                alternate_value: None,
            },
        ];

        let result = validator.validate_variables(&variables)?;

        assert!(!result.is_valid());
        assert_eq!(result.missing_vars, vec!["REQUIRED_VAR"]);
        assert_eq!(result.get_resolved("PG_PASS"), Some(&"secret".to_string()));
        assert_eq!(result.get_resolved("DB_PORT"), Some(&"5432".to_string()));

        Ok(())
    }
}
