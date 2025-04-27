use async_trait::async_trait;
use std::path::Path;

use super::{CommandResult, ExecutorError};

/// A trait for executing commands in a uniform way (local, SSH, etc.).
#[async_trait]
pub trait CommandExecutor {
    /// Execute a command and return a `CommandResult` containing stdout/stderr/exit code.
    async fn execute_command(&mut self, command: &str) -> Result<CommandResult, ExecutorError>;

    /// Close or clean up the executor (e.g., disconnect SSH).
    async fn close(&mut self) -> Result<(), ExecutorError>;
}

/// A trait for uploading files. SSH uses SFTP; local might do a filesystem copy.
/// Keep it separate so that executors that don't need file transfers aren't forced to implement it.
#[async_trait]
pub trait FileTransfer {
    async fn upload_file(&self, local_path: &Path, remote_path: &Path)
        -> Result<(), ExecutorError>;
}
