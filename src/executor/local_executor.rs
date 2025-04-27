use async_trait::async_trait;
use std::path::Path;
use std::time::Instant;
use tokio::process::Command;

use super::error::ExecutorError;
use super::traits::{CommandExecutor, FileTransfer};
use super::types::{CommandOutput, CommandResult};

pub struct LocalCommandExecutor;

impl Default for LocalCommandExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalCommandExecutor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CommandExecutor for LocalCommandExecutor {
    async fn execute_command(&mut self, command: &str) -> Result<CommandResult, ExecutorError> {
        let args: Vec<&str> = command.split_whitespace().collect();
        if args.is_empty() {
            return Err(ExecutorError::LocalError("No command provided".to_string()));
        }

        let program = args[0];
        let program_args = &args[1..];

        let start_time = Instant::now();

        let output = Command::new(program)
            .args(program_args)
            .output()
            .await
            .map_err(|e| ExecutorError::LocalError(e.to_string()))?;

        let mut cmd_output = CommandOutput::new();
        cmd_output.stdout = output.stdout;
        cmd_output.stderr = output.stderr;
        cmd_output.exit_code = output.status.code().unwrap_or_default() as u32;
        cmd_output.duration = start_time.elapsed();

        Ok(CommandResult {
            command: command.to_string(),
            output: cmd_output,
        })
    }

    async fn close(&mut self) -> Result<(), ExecutorError> {
        Ok(())
    }
}

#[async_trait]
impl FileTransfer for LocalCommandExecutor {
    async fn upload_file(
        &self,
        local_path: &Path,
        remote_path: &Path,
    ) -> Result<(), ExecutorError> {
        tokio::fs::copy(local_path, remote_path)
            .await
            .map_err(|e| ExecutorError::LocalError(e.to_string()))?;

        Ok(())
    }
}
