use crate::composer::errors::ComposerError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub type ComposerResult<T> = Result<T, ComposerError>;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ComposerVariables {
    pub name: String,
    pub required: bool,
    pub default_value: Option<String>,
    pub alternate_value: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PortMapping {
    pub mode: Option<String>,
    pub target: u16,
    pub published: String,
    pub protocol: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VolumeMapping {
    pub r#type: String, // 'bind' or 'volume', or 'tmpfs'
    pub source: Option<String>,
    pub target: String,
    pub read_only: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Service {
    pub container_name: Option<String>,
    pub image: Option<String>,
    pub build: Option<BuildConfig>,
    pub environment: Option<HashMap<String, String>>,
    pub ports: Option<Vec<PortMapping>>,
    pub volumes: Option<Vec<VolumeMapping>>,
    pub configs: Option<Vec<ConfigReference>>,
    #[serde(rename = "env_file")]
    pub env_file: Option<EnvFiles>,
    pub profiles: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum EnvFiles {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigReference {
    pub source: Option<PathBuf>,
    pub target: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BuildConfig {
    pub context: PathBuf,
    pub dockerfile: Option<PathBuf>,
    pub args: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ComposeFile {
    pub services: HashMap<String, Service>,
    pub volumes: Option<HashMap<String, Volume>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Volume {
    pub external: Option<bool>,
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ComposerConfig {
    pub project_dir: PathBuf,
    pub compose_files: Vec<PathBuf>,
    pub env_files: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ComposerOutput {
    pub consumed_env: HashMap<String, String>,
    pub missing_env: Vec<String>,
    pub exposed_ports: Vec<PortMapping>,
    pub volumes: Vec<VolumeMapping>,
    pub local_references: Vec<PathBuf>,
    pub resolved_compose_files: Vec<PathBuf>,
    pub resolved_project_dir: PathBuf,
    pub resolved_env_files: Vec<PathBuf>,
    pub available_profiles: Vec<String>,
    pub active_profiles: Vec<String>,
}

impl Default for ComposerOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl ComposerOutput {
    pub fn new() -> Self {
        Self {
            consumed_env: HashMap::new(),
            missing_env: Vec::new(),
            exposed_ports: Vec::new(),
            volumes: Vec::new(),
            local_references: Vec::new(),
            resolved_compose_files: Vec::new(),
            resolved_project_dir: PathBuf::new(),
            resolved_env_files: Vec::new(),
            available_profiles: Vec::new(),
            active_profiles: Vec::new(),
        }
    }
}
