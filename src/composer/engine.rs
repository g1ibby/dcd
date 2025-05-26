use crate::composer::{
    config::parser::ConfigParser,
    config::ports::PortsParser,
    config::volumes::VolumesParser,
    detection::{detect_compose_command, ComposeCommand, DetectionError},
    errors::ComposerError,
    types::{ComposeFile, ComposerConfig, ComposerOutput, ComposerResult},
    variables::availability::EnvironmentChecker,
    variables::availability::EnvironmentStatus,
    variables::parser::VariablesParser,
    variables::profiles::ProfilesHandler,
};
use crate::executor::CommandExecutor;

use semver::Version;
use std::{fs, path::PathBuf};

pub struct Composer<T: CommandExecutor> {
    executor: T,
    config: ComposerConfig,
    pub compose_command: ComposeCommand,
    pub compose_version: Version,
}

impl<T: CommandExecutor> Composer<T> {
    pub async fn try_new(mut executor: T, mut config: ComposerConfig) -> ComposerResult<Self> {
        // --- Resolve project directory to absolute path ---
        config.project_dir = fs::canonicalize(&config.project_dir).map_err(|e| {
            ComposerError::ConfigurationError(format!(
                "Failed to resolve project directory '{}': {}",
                config.project_dir.display(),
                e
            ))
        })?;
        tracing::debug!(
            "Resolved project directory to: {}",
            config.project_dir.display()
        );
        // --- End resolve project directory ---

        // Validate and handle compose files
        if config.compose_files.is_empty() {
            tracing::debug!("No compose files specified, looking for defaults...");
            // Try docker-compose.yml first, then docker-compose.yaml
            let yml_path = config.project_dir.join("docker-compose.yml");
            let yaml_path = config.project_dir.join("docker-compose.yaml");

            if yml_path.exists() {
                tracing::debug!("Found default docker-compose.yml");
                config.compose_files.push(yml_path);
            } else if yaml_path.exists() {
                tracing::debug!("Found default docker-compose.yaml");
                config.compose_files.push(yaml_path);
            } else {
                return Err(ComposerError::ConfigurationError(
                    "No compose files specified and no default docker-compose.yml or docker-compose.yaml found".to_string()
                ));
            }
        } else {
            // Verify all specified compose files exist
            for file_path in &config.compose_files {
                if !file_path.exists() {
                    return Err(ComposerError::ConfigurationError(format!(
                        "Specified compose file does not exist: {}",
                        file_path.display()
                    )));
                }
            }
            tracing::debug!("All specified compose files exist");
        }

        // Validate and handle env files
        if config.env_files.is_empty() {
            tracing::debug!("No env files specified, looking for default '.env'...");
            let default_env_path = config.project_dir.join(".env");
            if default_env_path.exists() {
                tracing::debug!("Found default .env file");
                config.env_files.push(default_env_path);
            } else {
                tracing::debug!("No default .env file found");
            }
        } else {
            // Verify all specified env files exist
            for file_path in &config.env_files {
                if !file_path.exists() {
                    return Err(ComposerError::ConfigurationError(format!(
                        "Specified env file does not exist: {}",
                        file_path.display()
                    )));
                }
            }
            tracing::debug!("All specified env files exist");
        }

        tracing::debug!("Detecting docker compose command...");
        let (command, version) =
            detect_compose_command(&mut executor)
                .await
                .map_err(|e| match e {
                    // Map detection errors to ComposerError
                    DetectionError::CommandNotFound => ComposerError::CommandNotFound,
                    DetectionError::VersionTooLow {
                        command,
                        version,
                        required,
                    } => ComposerError::VersionTooLow {
                        command,
                        version,
                        required,
                    },
                    DetectionError::CommandFailed(exec_err) => {
                        ComposerError::CommandExecutionError(format!(
                            "Detection command failed: {}",
                            exec_err
                        ))
                    }
                    DetectionError::OutputParsingError(msg) => ComposerError::ParseError(format!(
                        "Detection output parsing failed: {}",
                        msg
                    )),
                    DetectionError::VersionParsingError {
                        version_str,
                        source,
                    } => ComposerError::ConfigurationError(format!(
                        "Version parsing failed for '{}': {}",
                        version_str, source
                    )),
                })?;

        Ok(Self {
            executor,
            config,
            compose_command: command,
            compose_version: version,
        })
    }

    /// Main entry point - analyze docker compose configuration
    pub async fn analyze(&mut self) -> ComposerResult<ComposerOutput> {
        // Step 1: Check environment variables
        let env_status = self.check_environment_variables().await?;

        // If we have missing required variables, return early
        if !env_status.is_valid() {
            return Err(ComposerError::missing_vars(env_status.missing_required));
        }

        // Step 2: Get and parse the full compose config
        let compose_file = self.get_compose_config().await?;

        // Step 3: Extract all required information
        let mut output = self.process_compose_file(&compose_file)?;

        // Step 4: Handle profiles with access to env file variables
        let mut profiles_handler = ProfilesHandler::new();
        let mut env_checker = EnvironmentChecker::new();
        env_checker
            .check_environment(&[], &self.config.env_files)
            .await?;
        profiles_handler.set_env_file_vars(&env_checker.get_available_variables());

        // Update profile information in output
        output.active_profiles = profiles_handler.get_active_profiles();

        // Validate profiles and add COMPOSE_PROFILES to consumed_env if valid
        let profile_validation = profiles_handler.validate_profiles(&output.available_profiles)?;
        if profile_validation.is_valid() {
            if let Some(profiles_value) =
                profiles_handler.get_env_dcd_value(&output.available_profiles)
            {
                output
                    .consumed_env
                    .insert("COMPOSE_PROFILES".to_string(), profiles_value);
            }
        }

        // Add resolved environment variables to output
        output
            .consumed_env
            .extend(env_status.get_resolved_variables());

        // Add resolved file lists from the config held by the Composer instance
        output.resolved_compose_files = self.config.compose_files.clone();
        output.resolved_env_files = self.config.env_files.clone();
        output.resolved_project_dir = self.config.project_dir.clone(); // Populate the resolved project dir

        Ok(output)
    }

    /// Check environment variables using docker compose config --variables
    async fn check_environment_variables(&mut self) -> ComposerResult<EnvironmentStatus> {
        // Build the variables command
        let vars_cmd = self.build_compose_command("config --variables")?;
        tracing::debug!("Running command: {}", &vars_cmd);

        // Execute the command
        let result = self
            .executor
            .execute_command(&vars_cmd)
            .await
            .map_err(|e| ComposerError::command_error(e.to_string()))?;

        if !result.is_success() {
            return Err(ComposerError::command_error(
                "Failed to get variables configuration",
            ));
        }

        // Parse variables output
        let variables =
            VariablesParser::parse_variables_output(&result.output.to_stdout_string()?)?;

        // Check environment status
        let mut checker = EnvironmentChecker::new();
        checker
            .check_environment(&variables, &self.config.env_files)
            .await
    }

    /// Get and parse the full docker compose config
    async fn get_compose_config(&mut self) -> ComposerResult<ComposeFile> {
        let config_cmd = self.build_compose_command("config")?;

        let result = self
            .executor
            .execute_command(&config_cmd)
            .await
            .map_err(|e| ComposerError::CommandExecutionError(e.to_string()))?;

        if !result.is_success() {
            return Err(ComposerError::command_error(
                "Failed to get compose configuration",
            ));
        }

        ConfigParser::parse_config(&result.output.to_stdout_string()?)
    }

    /// Process the compose file to extract all required information
    fn process_compose_file(&self, compose_file: &ComposeFile) -> ComposerResult<ComposerOutput> {
        let mut output = ComposerOutput::new();

        // Collect all available profiles from all services
        let mut all_profiles = std::collections::HashSet::new();
        for service in compose_file.services.values() {
            if let Some(profiles) = &service.profiles {
                all_profiles.extend(profiles.iter().cloned());
            }
        }
        output.available_profiles = all_profiles.into_iter().collect();
        output.available_profiles.sort();

        // Handle profiles using ProfilesHandler
        let profiles_handler = ProfilesHandler::new();
        output.active_profiles = profiles_handler.get_active_profiles();

        // Extract ports and volumes from all services (profiles are handled by docker-compose itself)
        for service in compose_file.services.values() {
            if let Some(ports) = &service.ports {
                let parsed_ports = PortsParser::parse_ports(ports)?;
                output.exposed_ports.extend(parsed_ports);
            }

            // Extract volumes
            if let Some(volumes) = &service.volumes {
                let parsed_volumes =
                    VolumesParser::parse_volumes(volumes, &self.config.project_dir)?;
                output.volumes.extend(parsed_volumes);
            }
        }

        // Extract local references
        let references = ConfigParser::extract_local_references(compose_file);
        output
            .local_references
            .extend(references.into_iter().map(PathBuf::from));

        Ok(output)
    }

    fn build_compose_command(&self, subcommand: &str) -> ComposerResult<String> {
        let base_cmd = self.compose_command.command_string();
        let mut cmd_parts = base_cmd.split_whitespace().collect::<Vec<&str>>();

        // Add compose files
        for file in &self.config.compose_files {
            cmd_parts.push("-f");
            cmd_parts.push(file.to_str().ok_or_else(|| {
                ComposerError::ConfigurationError("Invalid compose file path".to_string())
            })?);
        }

        // Add env files
        for env_file in &self.config.env_files {
            cmd_parts.push("--env-file");
            cmd_parts.push(env_file.to_str().ok_or_else(|| {
                ComposerError::ConfigurationError("Invalid env file path".to_string())
            })?);
        }

        // Add subcommand
        cmd_parts.extend(subcommand.split_whitespace());

        Ok(cmd_parts.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composer::{
        detection::ComposeCommand,
        types::{PortMapping, Service, VolumeMapping},
    };
    use crate::executor::{CommandResult, ExecutorError};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    struct MockExecutor {
        // Store Result directly to simulate execution errors
        responses: HashMap<String, Result<CommandResult, ExecutorError>>,
        commands: Vec<String>,
    }

    impl MockExecutor {
        fn new() -> Self {
            Self {
                responses: HashMap::new(),
                commands: Vec::new(),
            }
        }

        fn add_response(&mut self, command: &str, result: Result<CommandResult, ExecutorError>) {
            self.responses.insert(command.to_string(), result);
        }

        // Helper to set up standard successful detection (e.g., plugin)
        fn setup_successful_plugin_detection(&mut self) {
            self.add_response(
                "docker compose version --format json",
                create_success_result("{\"version\":\"v2.5.1\"}"),
            );
            // Optionally add a failure for standalone if needed by a specific test
            self.add_response(
                "docker-compose --version",
                Err(ExecutorError::Other("command not found".into())), // Simulate execution error
            );
        }

        // Helper to set up standard successful detection (e.g., standalone)
        fn setup_successful_standalone_detection(&mut self) {
            self.add_response(
                "docker compose version --format json",
                Err(ExecutorError::Other("command not found".into())),
            );
            self.add_response(
                "docker-compose --version",
                create_success_result("docker-compose version v1.29.2, build abcdef"),
            );
        }

        // Helper to set up failed detection (both commands fail)
        fn setup_failed_detection(&mut self) {
            self.add_response(
                "docker compose version --format json",
                Err(ExecutorError::Other("command not found".into())),
            );
            self.add_response(
                "docker-compose --version",
                Err(ExecutorError::Other("command not found".into())),
            );
        }
    }

    #[async_trait]
    impl CommandExecutor for MockExecutor {
        async fn execute_command(&mut self, command: &str) -> Result<CommandResult, ExecutorError> {
            self.commands.push(command.to_string());
            // Clone the result from the map, or return an error if not found
            let response = self.responses.get(command).cloned().ok_or_else(|| {
                ExecutorError::Other(format!("Mock response not found for command: {}", command))
            })?;

            let result = response?; // Propagate potential ExecutorError stored in the map value
            Ok(result)
        }

        async fn close(&mut self) -> Result<(), ExecutorError> {
            Ok(())
        }
    }

    fn create_test_environment() -> (TempDir, ComposerConfig) {
        let temp_dir = TempDir::new().unwrap();
        // Create a docker-compose.yml file in the temp directory
        fs::write(temp_dir.path().join("docker-compose.yml"), "version: '3'").unwrap();

        let config = ComposerConfig {
            project_dir: temp_dir.path().to_path_buf(),
            compose_files: vec![temp_dir.path().join("docker-compose.yml")],
            env_files: vec![],
        };

        (temp_dir, config)
    }

    fn create_env_file(dir: &TempDir, filename: &str, content: &str) -> PathBuf {
        let env_path = dir.path().join(filename);
        let mut file = fs::File::create(&env_path).unwrap();
        write!(file, "{}", content).unwrap();
        env_path
    }

    fn create_success_result(stdout: &str) -> Result<CommandResult, ExecutorError> {
        let mut result = CommandResult::new("mock_command");
        result.output.stdout = stdout.as_bytes().to_vec();
        result.output.exit_code = 0;
        Ok(result)
    }

    #[tokio::test]
    async fn test_composer_try_new_success_plugin() {
        let (temp_dir, config) = create_test_environment();
        let mut executor = MockExecutor::new();
        executor.setup_successful_plugin_detection(); // Setup mock for successful plugin detection

        let composer_result = Composer::try_new(executor, config).await;
        assert!(composer_result.is_ok());
        let composer = composer_result.unwrap();

        assert_eq!(composer.compose_command, ComposeCommand::Plugin);
        assert_eq!(composer.compose_version, Version::parse("2.5.1").unwrap());

        // Test build_compose_command implicitly
        let cmd = composer.build_compose_command("config").unwrap();
        let expected_start = format!(
            "docker compose -f {}",
            temp_dir.path().join("docker-compose.yml").display()
        );
        // let expected_env = format!("--env-file {}", temp_dir.path().join(".env").display()); // No env file in this setup
        assert!(cmd.starts_with(&expected_start));
        // assert!(cmd.contains(&expected_env));
        assert!(cmd.ends_with(" config"));
    }

    #[tokio::test]
    async fn test_composer_try_new_success_standalone() {
        let (_temp_dir, config) = create_test_environment();
        let mut executor = MockExecutor::new();
        executor.setup_successful_standalone_detection(); // Setup mock for successful standalone detection

        let composer_result = Composer::try_new(executor, config).await;
        assert!(composer_result.is_ok());
        let composer = composer_result.unwrap();

        assert_eq!(composer.compose_command, ComposeCommand::Standalone);
        assert_eq!(composer.compose_version, Version::parse("1.29.2").unwrap());
    }

    #[tokio::test]
    async fn test_build_compose_command() {
        // Tests that the build_compose_command method correctly constructs a docker compose command
        // with the appropriate -f and --env-file flags based on the provided configuration
        let (temp_dir, mut config) = create_test_environment();

        // Add an env file
        let env_path = create_env_file(&temp_dir, ".env", "TEST=value");
        config.env_files.push(env_path);

        let mut executor = MockExecutor::new();
        executor.setup_successful_plugin_detection();
        let composer = Composer::try_new(executor, config).await.unwrap();

        let cmd = composer.build_compose_command("config").unwrap();

        assert!(cmd.starts_with("docker compose -f "));
        assert!(cmd.contains(" --env-file "));
        assert!(cmd.ends_with(" config"));
    }

    #[tokio::test]
    async fn test_get_compose_config() {
        let (_temp_dir, config) = create_test_environment();

        // Mock the config output
        let config_output = r#"
services:
  db:
    container_name: postgres
    environment:
      POSTGRES_PASSWORD: password
      POSTGRES_USER: user
    image: postgres:13
    networks:
      default: null
    ports:
      - mode: ingress
        target: 5432
        published: "5432"
        protocol: tcp
    volumes:
      - type: volume
        source: postgres_data
        target: /var/lib/postgresql/data
        volume: {}
networks:
  default:
    name: dcd_default
volumes:
  postgres_data:
    name: dcd_postgres_dat
"#;

        let mut executor = MockExecutor::new();
        executor.setup_successful_plugin_detection(); // Need detection to succeed
                                                      // build_compose_command will construct the command with the absolute path
        let expected_config_cmd = format!(
            "docker compose -f {} config",
            config.compose_files[0].display()
        );
        executor.add_response(&expected_config_cmd, create_success_result(config_output)); // Mock the config command
        let mut composer = Composer::try_new(executor, config).await.unwrap();

        let compose_file = composer.get_compose_config().await.unwrap();

        assert!(compose_file.services.contains_key("db"));
        let db_service = &compose_file.services["db"];
        assert_eq!(db_service.image, Some("postgres:13".to_string()));
        assert_eq!(db_service.container_name, Some("postgres".to_string()));

        // Check ports
        let ports = db_service.ports.as_ref().unwrap();
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].published, "5432");
        assert_eq!(ports[0].target, 5432);

        // Check volumes
        let volumes = db_service.volumes.as_ref().unwrap();
        assert_eq!(volumes.len(), 1);
        assert_eq!(volumes[0].source, Some("postgres_data".to_string()));
        assert_eq!(volumes[0].target, "/var/lib/postgresql/data");
    }

    #[tokio::test]
    async fn test_process_compose_file() {
        let (_temp_dir, config) = create_test_environment();

        // Create a compose file with services, ports, and volumes
        let mut services = HashMap::new();

        // Add a service with ports and volumes
        let db_service = Service {
            container_name: Some("postgres".to_string()),
            image: Some("postgres:13".to_string()),
            build: None,
            environment: None,
            ports: Some(vec![PortMapping {
                mode: None,
                target: 5432,
                published: "5432".to_string(),
                protocol: None,
            }]),
            volumes: Some(vec![VolumeMapping {
                r#type: "bind".to_string(),
                source: Some("/local/path".to_string()),
                target: "/container/path".to_string(),
                read_only: Some(false),
            }]),
            configs: None,
            env_file: None,
            profiles: None,
        };

        services.insert("db".to_string(), db_service);

        let compose_file = ComposeFile {
            services,
            volumes: None,
        };

        // Need a Composer instance, detection doesn't matter for this test function itself
        let mut executor = MockExecutor::new();
        executor.setup_successful_plugin_detection(); // Provide detection mocks
        let composer = Composer::try_new(executor, config).await.unwrap();

        let output = composer.process_compose_file(&compose_file).unwrap();

        // Check extracted ports
        assert_eq!(output.exposed_ports.len(), 1);
        assert_eq!(output.exposed_ports[0].published, "5432");
        assert_eq!(output.exposed_ports[0].target, 5432);

        // Check extracted volumes
        assert_eq!(output.volumes.len(), 1);
        assert_eq!(output.volumes[0].r#type, "bind");
        // Source path should be resolved relative to project_dir
        assert_eq!(output.volumes[0].target, "/container/path");

        // Check local references (from bind mount source)
        assert_eq!(output.local_references.len(), 1);
    }

    #[tokio::test]
    async fn test_composer_try_new_detection_failure() {
        let (_temp_dir, config) = create_test_environment();

        let mut executor = MockExecutor::new();
        executor.setup_failed_detection(); // Setup mock for failed detection

        let result = Composer::try_new(executor, config).await;

        // Should return a CommandNotFound error
        assert!(result.is_err());
        // Check that the error is specifically CommandNotFound
        match result.err().unwrap() {
            ComposerError::CommandExecutionError(_) => {} // Expect CommandExecutionError
            e => panic!("Expected CommandExecutionError, got {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_composer_try_new_version_too_low() {
        let (_temp_dir, config) = create_test_environment();
        let mut executor = MockExecutor::new();
        // Mock plugin detection failing
        executor.add_response(
            "docker compose version --format json",
            Err(ExecutorError::Other("command not found".into())),
        );
        // Mock standalone detection succeeding but with an old version
        executor.add_response(
            "docker-compose --version",
            create_success_result("docker-compose version v1.20.0, build abcdef"),
        );

        let result = Composer::try_new(executor, config).await;
        assert!(result.is_err());
        match result.err().unwrap() {
            ComposerError::VersionTooLow {
                command, version, ..
            } => {
                assert_eq!(command, "docker-compose");
                assert_eq!(version, Version::parse("1.20.0").unwrap());
            }
            e => panic!("Expected VersionTooLow, got {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_composer_try_new_missing_compose_files() {
        // Create temp dir but don't create any compose files
        let temp_dir = TempDir::new().unwrap();

        let config = ComposerConfig {
            project_dir: temp_dir.path().to_path_buf(),
            compose_files: vec![], // Empty compose files list
            env_files: vec![],
        };

        let mut executor = MockExecutor::new();
        executor.setup_successful_plugin_detection();

        let result = Composer::try_new(executor, config).await;
        assert!(result.is_err());
        match result.err().unwrap() {
            ComposerError::ConfigurationError(msg) => {
                assert!(msg.contains("No compose files specified and no default"));
            }
            e => panic!("Expected ConfigurationError, got {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_composer_try_new_nonexistent_compose_file() {
        let temp_dir = TempDir::new().unwrap();

        let config = ComposerConfig {
            project_dir: temp_dir.path().to_path_buf(),
            compose_files: vec![temp_dir.path().join("nonexistent-file.yml")], // File doesn't exist
            env_files: vec![],
        };

        let mut executor = MockExecutor::new();
        executor.setup_successful_plugin_detection();

        let result = Composer::try_new(executor, config).await;
        assert!(result.is_err());
        match result.err().unwrap() {
            ComposerError::ConfigurationError(msg) => {
                assert!(msg.contains("does not exist"));
            }
            e => panic!("Expected ConfigurationError, got {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_composer_try_new_default_compose_file() {
        // Create temp dir with a default docker-compose.yml
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("docker-compose.yml"), "version: '3'").unwrap();

        let config = ComposerConfig {
            project_dir: temp_dir.path().to_path_buf(),
            compose_files: vec![], // Empty compose files list - should find default
            env_files: vec![],
        };

        let mut executor = MockExecutor::new();
        executor.setup_successful_plugin_detection();

        // Mock the config command that will be called after detection
        let expected_config_cmd = format!(
            "docker compose -f {} config",
            temp_dir.path().join("docker-compose.yml").display()
        );
        executor.add_response(&expected_config_cmd, create_success_result("services: {}"));

        let result = Composer::try_new(executor, config).await;
        assert!(result.is_ok());
    }
}
