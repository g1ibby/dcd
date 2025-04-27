use crate::executor::{CommandExecutor, ExecutorError};
use semver::{Version, VersionReq};
use serde::Deserialize;
use thiserror::Error;

// Minimum required versions
const MIN_PLUGIN_VERSION: &str = ">= 2.0.0";
const MIN_STANDALONE_VERSION: &str = ">= 1.28.0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeCommand {
    Plugin,
    Standalone,
}

impl ComposeCommand {
    pub fn command_string(&self) -> &'static str {
        match self {
            ComposeCommand::Plugin => "docker compose",
            ComposeCommand::Standalone => "docker-compose",
        }
    }
}

#[derive(Error, Debug)]
pub enum DetectionError {
    #[error("Neither 'docker compose' nor 'docker-compose' could be found or executed.")]
    CommandNotFound,
    #[error(
        "Detected {command} version {version} is below the minimum required version {required}."
    )]
    VersionTooLow {
        command: String,
        version: Version,
        required: String,
    },
    #[error("Failed to execute command: {0}")]
    CommandFailed(#[from] ExecutorError),
    #[error("Failed to parse command output: {0}")]
    OutputParsingError(String),
    #[error("Failed to parse version string '{version_str}': {source}")]
    VersionParsingError {
        version_str: String,
        source: semver::Error,
    },
}

// Helper struct for parsing `docker compose version --format json`
#[derive(Deserialize, Debug)]
struct ComposeVersionJson {
    #[serde(rename = "version")]
    version: String,
}

/// Detects the available docker compose command and checks its version.
pub async fn detect_compose_command<E: CommandExecutor>(
    executor: &mut E,
) -> Result<(ComposeCommand, Version), DetectionError> {
    // --- Try 'docker compose' (Plugin) first ---
    let plugin_cmd = "docker compose version --format json";
    match executor.execute_command(plugin_cmd).await {
        Ok(result) if result.is_success() => {
            let stdout = result
                .output
                .to_stdout_string()
                .map_err(|e| DetectionError::OutputParsingError(e.to_string()))?;
            // Parse JSON output
            let parsed_json: ComposeVersionJson = serde_json::from_str(&stdout).map_err(|e| {
                DetectionError::OutputParsingError(format!(
                    "JSON parse failed: {}, Output: {}",
                    e, stdout
                ))
            })?;

            let version_str = parsed_json.version;
            let version_str_trimmed = version_str.trim_start_matches('v');
            let version = Version::parse(version_str_trimmed).map_err(|e| {
                DetectionError::VersionParsingError {
                    version_str: version_str.clone(),
                    source: e,
                }
            })?;

            let req = VersionReq::parse(MIN_PLUGIN_VERSION).unwrap();
            if req.matches(&version) {
                tracing::debug!("Detected docker compose (plugin) version {}", version);
                return Ok((ComposeCommand::Plugin, version));
            } else {
                tracing::warn!(
                    "Detected docker compose (plugin) version {} is below minimum {}",
                    version,
                    MIN_PLUGIN_VERSION
                );
            }
        }
        Ok(result) => {
            tracing::debug!(
                "'docker compose version' failed: {}",
                result.output.to_stderr_string().unwrap_or_default()
            );
        }
        Err(e) => {
            tracing::debug!("Error executing 'docker compose version': {}", e);
        }
    }

    // --- Try 'docker-compose' (Standalone) second ---
    let standalone_cmd = "docker-compose --version";
    match executor.execute_command(standalone_cmd).await {
        Ok(result) if result.is_success() => {
            let output = result
                .output
                .to_stdout_string()
                .map_err(|e| DetectionError::OutputParsingError(e.to_string()))?;
            // Example outputs: "docker-compose version 1.29.2, build 5becea4c" or "Docker Compose version v2.10.0"
            let version_str = output
                .split_whitespace()
                .find(|s| {
                    s.starts_with(|c: char| c.is_ascii_digit())
                        || (s.starts_with('v')
                            && s.len() > 1
                            && s[1..].starts_with(|c: char| c.is_ascii_digit()))
                })
                .map(|s| s.trim_start_matches('v')) // Remove leading 'v' if present
                .ok_or_else(|| {
                    DetectionError::OutputParsingError(format!(
                        "Could not find version string in output: {}",
                        output
                    ))
                })?;
            // Strip trailing non-digit characters (like commas)
            let version_str = version_str.trim_end_matches(|c: char| !c.is_ascii_digit());

            let version =
                Version::parse(version_str).map_err(|e| DetectionError::VersionParsingError {
                    version_str: version_str.to_string(),
                    source: e,
                })?;

            let req = VersionReq::parse(MIN_STANDALONE_VERSION).unwrap(); // Should not fail
            if req.matches(&version) {
                tracing::debug!("Detected docker-compose (standalone) version {}", version);
                Ok((ComposeCommand::Standalone, version))
            } else {
                Err(DetectionError::VersionTooLow {
                    command: "docker-compose".to_string(),
                    version,
                    required: MIN_STANDALONE_VERSION.to_string(),
                })
            }
        }
        Ok(result) => {
            // Command ran but failed
            tracing::debug!(
                "'docker-compose --version' failed: {}",
                result.output.to_stderr_string().unwrap_or_default()
            );
            // If plugin also failed, now we report not found
            Err(DetectionError::CommandNotFound)
        }
        Err(e) => {
            // Executor error
            tracing::debug!("Error executing 'docker-compose --version': {}", e);
            // If plugin also failed, now we report not found (or the executor error)
            Err(DetectionError::CommandFailed(e))
        }
    }
}
