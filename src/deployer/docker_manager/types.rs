use super::error::DockerError;

pub struct DockerVersion {
    pub version: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinuxDistro {
    Debian,
    Ubuntu,
    Unknown(String),
}

pub type DockerResult<T> = Result<T, DockerError>;
