use super::{SyncPair, SyncPlan};
use crate::deployer::types::{ComposeExec, DeployError, DeployResult};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncReadExt;

#[derive(Debug, Default)]
pub struct FileSyncStatus {
    pub files_synced: Vec<PathBuf>,
    pub files_skipped: Vec<PathBuf>,
    pub files_failed: Vec<(PathBuf, String)>,
}

pub struct FileSync<'a> {
    executor: &'a mut (dyn ComposeExec + Send),
    remote_root: PathBuf,
}

impl<'a> FileSync<'a> {
    pub fn new(executor: &'a mut (dyn ComposeExec + Send), remote_root: PathBuf) -> Self {
        Self {
            executor,
            remote_root,
        }
    }

    /// Synchronize files according to the sync plan
    pub async fn sync_files(&mut self, plan: &SyncPlan) -> DeployResult<FileSyncStatus> {
        let mut status = FileSyncStatus::default();

        // Ensure remote directory exists
        self.ensure_remote_directory().await?;

        // Sync compose files first
        for pair in &plan.compose_files {
            self.sync_file(pair, &mut status).await?;
        }

        // Sync env files
        for pair in &plan.env_files {
            self.sync_file(pair, &mut status).await?;
        }

        // Sync reference files
        for pair in &plan.reference_files {
            self.sync_file(pair, &mut status).await?;
        }

        // Sync other files
        for pair in &plan.files {
            self.sync_file(pair, &mut status).await?;
        }

        Ok(status)
    }

    async fn sync_file(
        &mut self,
        pair: &SyncPair,
        status: &mut FileSyncStatus,
    ) -> DeployResult<()> {
        if pair.is_directory {
            Box::pin(self.sync_directory(pair, status)).await?;
        } else if self.should_sync_file(pair).await? {
            match self
                .executor
                .upload_file(&pair.local_path, &pair.remote_path)
                .await
            {
                Ok(_) => status.files_synced.push(pair.local_path.clone()),
                Err(e) => {
                    status
                        .files_failed
                        .push((pair.local_path.clone(), e.to_string()));
                    return Err(DeployError::FileSync(format!(
                        "Failed to sync file {}: {}",
                        pair.local_path.display(),
                        e
                    )));
                }
            }
        } else {
            status.files_skipped.push(pair.local_path.clone());
        }
        Ok(())
    }

    async fn sync_directory(
        &mut self,
        pair: &SyncPair,
        status: &mut FileSyncStatus,
    ) -> DeployResult<()> {
        // Create remote directory
        let mkdir_cmd = format!("mkdir -p {}", pair.remote_path.display());
        self.executor
            .execute_command(&mkdir_cmd)
            .await
            .map_err(|e| {
                DeployError::FileSync(format!("Failed to create remote directory: {}", e))
            })?;

        // Recursively sync directory contents
        let mut entries = fs::read_dir(&pair.local_path)
            .await
            .map_err(|e| DeployError::FileSync(format!("Failed to read directory: {}", e)))?;

        while let Ok(Some(entry)) = entries.next_entry().await {
            let local_path = entry.path();
            let relative_path = local_path
                .strip_prefix(&pair.local_path)
                .map_err(|e| DeployError::FileSync(e.to_string()))?;
            let remote_path = pair.remote_path.join(relative_path);

            let is_dir = entry
                .file_type()
                .await
                .map_err(|e| DeployError::FileSync(e.to_string()))?
                .is_dir();

            let sub_pair = SyncPair::new(local_path, remote_path, is_dir);
            self.sync_file(&sub_pair, status).await?;
        }

        Ok(())
    }

    async fn should_sync_file(&mut self, pair: &SyncPair) -> DeployResult<bool> {
        // Check if remote file exists and compare checksums
        let check_cmd = format!("sha256sum {}", pair.remote_path.display());
        match self.executor.execute_command(&check_cmd).await {
            Ok(result) if result.is_success() => {
                let stdout = result
                    .output
                    .to_stdout_string()
                    .map_err(|e| DeployError::FileSync(e.to_string()))?;

                let remote_sum = stdout
                    .split_whitespace()
                    .next()
                    .ok_or_else(|| DeployError::FileSync("Invalid checksum output".into()))?;

                let local_sum = sha256_file(&pair.local_path).await?;
                Ok(local_sum != remote_sum)
            }
            _ => Ok(true), // File doesn't exist or error reading it, should sync
        }
    }

    async fn ensure_remote_directory(&mut self) -> DeployResult<()> {
        let cmd = format!("mkdir -p {}", self.remote_root.display());
        self.executor.execute_command(&cmd).await.map_err(|e| {
            DeployError::FileSync(format!("Failed to create remote directory: {}", e))
        })?;
        Ok(())
    }
}

async fn sha256_file(path: impl AsRef<Path>) -> DeployResult<String> {
    let mut file = fs::File::open(path)
        .await
        .map_err(|e| DeployError::FileSync(format!("Failed to open file: {}", e)))?;

    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .await
        .map_err(|e| DeployError::FileSync(format!("Failed to read file: {}", e)))?;

    let mut hasher = Sha256::new();
    hasher.update(&buffer);
    Ok(format!("{:x}", hasher.finalize()))
}
