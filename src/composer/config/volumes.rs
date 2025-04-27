use crate::composer::types::{ComposerResult, VolumeMapping};
use std::path::Path;

pub struct VolumesParser;

#[derive(Debug, Clone)]
pub struct ParsedVolume {
    pub source: String,
    pub target: String,
    pub volume_type: VolumeType,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VolumeType {
    Named,
    Bind,
    Tmpfs,
}

impl VolumesParser {
    pub fn parse_volumes(
        volumes: &[VolumeMapping],
        _project_dir: &Path,
    ) -> ComposerResult<Vec<VolumeMapping>> {
        Ok(volumes.to_vec())
    }

    /// Get all local paths that need to exist for bind mounts
    pub fn get_required_local_paths(volumes: &[ParsedVolume]) -> Vec<String> {
        volumes
            .iter()
            .filter(|v| v.volume_type == VolumeType::Bind)
            .map(|v| v.source.clone())
            .collect()
    }

    /// Get all named volumes that need to be created
    pub fn get_named_volumes(volumes: &[ParsedVolume]) -> Vec<String> {
        volumes
            .iter()
            .filter(|v| v.volume_type == VolumeType::Named)
            .map(|v| v.source.clone())
            .collect()
    }
}
