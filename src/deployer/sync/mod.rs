mod env;
mod files;

use std::path::{Path, PathBuf};

pub use env::EnvFileManager;
pub use files::{FileSync, FileSyncStatus};

/// Represents a file pair for synchronization
#[derive(Debug, Clone)]
pub struct SyncPair {
    pub local_path: PathBuf,
    pub remote_path: PathBuf,
    pub is_directory: bool,
}

impl SyncPair {
    pub fn new(local: impl AsRef<Path>, remote: impl AsRef<Path>, is_directory: bool) -> Self {
        Self {
            local_path: local.as_ref().to_path_buf(),
            remote_path: remote.as_ref().to_path_buf(),
            is_directory,
        }
    }
}

/// Collection of files that need to be synchronized
#[derive(Debug, Default)]
pub struct SyncPlan {
    pub files: Vec<SyncPair>,
    pub env_files: Vec<SyncPair>,
    pub compose_files: Vec<SyncPair>,
    pub reference_files: Vec<SyncPair>,
}

impl SyncPlan {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_file(
        &mut self,
        local: impl AsRef<Path>,
        remote: impl AsRef<Path>,
        is_directory: bool,
    ) {
        self.files.push(SyncPair::new(local, remote, is_directory));
    }

    pub fn add_env_file(&mut self, local: impl AsRef<Path>, remote: impl AsRef<Path>) {
        self.env_files.push(SyncPair::new(local, remote, false));
    }

    pub fn add_compose_file(&mut self, local: impl AsRef<Path>, remote: impl AsRef<Path>) {
        self.compose_files.push(SyncPair::new(local, remote, false));
    }

    pub fn add_reference(
        &mut self,
        local: impl AsRef<Path>,
        remote: impl AsRef<Path>,
        is_directory: bool,
    ) {
        self.reference_files
            .push(SyncPair::new(local, remote, is_directory));
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
            && self.env_files.is_empty()
            && self.compose_files.is_empty()
            && self.reference_files.is_empty()
    }
}
