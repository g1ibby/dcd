use std::collections::HashMap;

use crate::composer::types::ComposerResult;

pub struct ProfilesHandler {
    system_env: HashMap<String, String>,
    env_file_vars: HashMap<String, String>,
}

impl Default for ProfilesHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ProfilesHandler {
    pub fn new() -> Self {
        Self {
            system_env: std::env::vars().collect(),
            env_file_vars: HashMap::new(),
        }
    }

    /// Set env file variables (usually called by VariablesValidator)
    pub fn set_env_file_vars(&mut self, env_vars: &HashMap<String, String>) {
        self.env_file_vars = env_vars.clone();
    }

    /// Get the active profiles from COMPOSE_PROFILES environment variable
    pub fn get_active_profiles(&self) -> Vec<String> {
        self.get_compose_profiles_value()
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Get the COMPOSE_PROFILES value if it exists
    pub fn get_compose_profiles_value(&self) -> Option<String> {
        // Check system environment first
        if let Some(value) = self.system_env.get("COMPOSE_PROFILES") {
            return Some(value.clone());
        }

        // Then check .env file
        if let Some(value) = self.env_file_vars.get("COMPOSE_PROFILES") {
            return Some(value.clone());
        }

        None
    }

    /// Validate that active profiles exist in available profiles
    pub fn validate_profiles(
        &self,
        available_profiles: &[String],
    ) -> ComposerResult<ProfileValidationResult> {
        let active_profiles = self.get_active_profiles();
        let mut result = ProfileValidationResult::new();

        result.active_profiles = active_profiles.clone();
        result.available_profiles = available_profiles.to_vec();

        // Check if all active profiles are available
        for profile in &active_profiles {
            if !available_profiles.contains(profile) {
                result.invalid_profiles.push(profile.clone());
            }
        }

        Ok(result)
    }

    /// Check if COMPOSE_PROFILES should be included in .env.dcd
    pub fn should_include_in_env_dcd(&self, available_profiles: &[String]) -> bool {
        if let Some(profiles_value) = self.get_compose_profiles_value() {
            let active_profiles: Vec<String> = profiles_value
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            // Only include if all active profiles are valid
            active_profiles
                .iter()
                .all(|profile| available_profiles.contains(profile))
        } else {
            false
        }
    }

    /// Get the value to write to .env.dcd if profiles should be included
    pub fn get_env_dcd_value(&self, available_profiles: &[String]) -> Option<String> {
        if self.should_include_in_env_dcd(available_profiles) {
            self.get_compose_profiles_value()
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProfileValidationResult {
    pub active_profiles: Vec<String>,
    pub available_profiles: Vec<String>,
    pub invalid_profiles: Vec<String>,
}

impl Default for ProfileValidationResult {
    fn default() -> Self {
        Self::new()
    }
}

impl ProfileValidationResult {
    pub fn new() -> Self {
        Self {
            active_profiles: Vec::new(),
            available_profiles: Vec::new(),
            invalid_profiles: Vec::new(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.invalid_profiles.is_empty()
    }

    pub fn has_active_profiles(&self) -> bool {
        !self.active_profiles.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_active_profiles_from_system_env() {
        std::env::set_var("COMPOSE_PROFILES", "dev,test");

        let handler = ProfilesHandler::new();
        let profiles = handler.get_active_profiles();

        assert_eq!(profiles, vec!["dev", "test"]);

        std::env::remove_var("COMPOSE_PROFILES");
    }

    #[test]
    fn test_get_active_profiles_from_env_file() {
        let mut env_vars = HashMap::new();
        env_vars.insert("COMPOSE_PROFILES".to_string(), "prod,staging".to_string());

        let mut handler = ProfilesHandler::new();
        handler.set_env_file_vars(&env_vars);

        let profiles = handler.get_active_profiles();
        assert_eq!(profiles, vec!["prod", "staging"]);
    }

    #[test]
    fn test_validate_profiles_valid() {
        let mut env_vars = HashMap::new();
        env_vars.insert("COMPOSE_PROFILES".to_string(), "dev,test".to_string());

        let mut handler = ProfilesHandler::new();
        handler.set_env_file_vars(&env_vars);

        let available = vec!["dev".to_string(), "test".to_string(), "prod".to_string()];
        let result = handler.validate_profiles(&available).unwrap();

        assert!(result.is_valid());
        assert_eq!(result.active_profiles, vec!["dev", "test"]);
        assert!(result.invalid_profiles.is_empty());
    }

    #[test]
    fn test_validate_profiles_invalid() {
        let mut env_vars = HashMap::new();
        env_vars.insert("COMPOSE_PROFILES".to_string(), "dev,invalid".to_string());

        let mut handler = ProfilesHandler::new();
        handler.set_env_file_vars(&env_vars);

        let available = vec!["dev".to_string(), "test".to_string()];
        let result = handler.validate_profiles(&available).unwrap();

        assert!(!result.is_valid());
        assert_eq!(result.invalid_profiles, vec!["invalid"]);
    }

    #[test]
    fn test_should_include_in_env_dcd() {
        let mut env_vars = HashMap::new();
        env_vars.insert("COMPOSE_PROFILES".to_string(), "dev,test".to_string());

        let mut handler = ProfilesHandler::new();
        handler.set_env_file_vars(&env_vars);

        let available = vec!["dev".to_string(), "test".to_string(), "prod".to_string()];
        assert!(handler.should_include_in_env_dcd(&available));

        let limited_available = vec!["dev".to_string()];
        assert!(!handler.should_include_in_env_dcd(&limited_available));
    }
}
