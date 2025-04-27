use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::HashMap;
use std::string::FromUtf8Error;
use std::time::{Duration, SystemTime};
use thiserror::Error;

/// Different ways to interpret command output
#[derive(Debug)]
pub enum OutputFormat {
    Raw,
    Lines,
    KeyValue,
    Json,
}

/// Errors that can occur when processing or parsing command output
#[derive(Debug, Error)]
pub enum OutputError {
    #[error("UTF-8 conversion error: {0}")]
    Utf8Error(#[from] FromUtf8Error),

    #[error("Output is empty")]
    EmptyOutput,

    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Output exceeds maximum size: {size} bytes")]
    OutputTooLarge { size: usize },
}

/// Contains the raw output (stdout/stderr), exit code, timing information, etc.
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: u32,
    pub timestamp: SystemTime,
    pub duration: Duration,
}

impl Default for CommandOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandOutput {
    const MAX_OUTPUT_SIZE: usize = 10 * 1024 * 1024; // 10 MB

    pub fn new() -> Self {
        Self {
            stdout: vec![],
            stderr: vec![],
            exit_code: 0,
            timestamp: SystemTime::now(),
            duration: Duration::default(),
        }
    }

    /// Update `duration` based on time elapsed since `timestamp`.
    pub fn stop_timing(&mut self) {
        if let Ok(elapsed) = self.timestamp.elapsed() {
            self.duration = elapsed;
        }
    }

    /// Convert stdout bytes to UTF-8 string
    pub fn to_stdout_string(&self) -> Result<String, OutputError> {
        if self.stdout.len() > Self::MAX_OUTPUT_SIZE {
            return Err(OutputError::OutputTooLarge {
                size: self.stdout.len(),
            });
        }
        Ok(String::from_utf8(self.stdout.clone())?)
    }

    /// Convert stderr bytes to UTF-8 string
    pub fn to_stderr_string(&self) -> Result<String, OutputError> {
        if self.stderr.len() > Self::MAX_OUTPUT_SIZE {
            return Err(OutputError::OutputTooLarge {
                size: self.stderr.len(),
            });
        }
        Ok(String::from_utf8(self.stderr.clone())?)
    }

    /// Split stdout into lines (trim and filter out empty lines).
    pub fn stdout_lines(&self) -> Result<Vec<String>, OutputError> {
        Ok(self
            .to_stdout_string()?
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect())
    }

    /// Check if stdout contains a given pattern (simple substring).
    pub fn contains(&self, pattern: &str) -> bool {
        self.to_stdout_string()
            .map(|s| s.contains(pattern))
            .unwrap_or(false)
    }
}

/// Wraps the command that was run plus its resulting output.
#[derive(Debug, Clone)]
pub struct CommandResult {
    pub command: String,
    pub output: CommandOutput,
}

impl CommandResult {
    pub fn new(command: &str) -> Self {
        Self {
            command: command.to_string(),
            output: CommandOutput::new(),
        }
    }

    pub fn is_success(&self) -> bool {
        self.output.exit_code == 0
    }

    /// Parse stdout as JSON into a custom type
    pub fn parse_json<T: DeserializeOwned>(&self) -> Result<T, OutputError> {
        serde_json::from_slice(&self.output.stdout).map_err(OutputError::JsonError)
    }

    /// Parse stdout lines as key-value pairs (assumes `key: value` format)
    pub fn parse_key_value(&self) -> Result<HashMap<String, String>, OutputError> {
        let mut map = HashMap::new();
        for line in self.output.stdout_lines()? {
            if let Some((k, v)) = line.split_once(':') {
                map.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        Ok(map)
    }

    /// Duration from command start to completion
    pub fn duration(&self) -> Duration {
        self.output.duration
    }

    /// Dynamically process output in different formats
    pub fn process_output(&self, format: OutputFormat) -> Result<ProcessedOutput, OutputError> {
        match format {
            OutputFormat::Raw => Ok(ProcessedOutput::Raw(self.output.to_stdout_string()?)),
            OutputFormat::Lines => Ok(ProcessedOutput::Lines(self.output.stdout_lines()?)),
            OutputFormat::KeyValue => Ok(ProcessedOutput::KeyValue(self.parse_key_value()?)),
            OutputFormat::Json => {
                let json: Value = self.parse_json()?;
                Ok(ProcessedOutput::Json(json))
            }
        }
    }
}

/// Encapsulates possible result types after formatting stdout
#[derive(Debug)]
pub enum ProcessedOutput {
    Raw(String),
    Lines(Vec<String>),
    KeyValue(HashMap<String, String>),
    Json(Value),
}
