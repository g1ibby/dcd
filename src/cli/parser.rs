use super::{analyze, destroy, status, up};
use clap::{ArgAction, Parser, Subcommand};
use std::path::PathBuf;

const VERSION_INFO: &str = env!("DCD_BUILD_VERSION");

#[derive(Parser, Debug)]
#[command(name = "dcd")]
#[command(about = "Docker Compose Deployment tool", long_about = None, version = VERSION_INFO)]
#[command(propagate_version = true)]
pub struct Cli {
    /// Docker compose file(s)
    #[arg(short = 'f', long = "file")]
    pub compose_files: Vec<PathBuf>,

    /// Environment file(s)
    #[arg(short = 'e', long = "env-file")]
    pub env_files: Vec<PathBuf>,

    /// SSH private key path (auto-detects ~/.ssh/id_rsa or ~/.ssh/id_ed25519 if not specified)
    #[arg(short = 'i', long = "identity")]
    pub identity_file: Option<PathBuf>,

    /// Remote working directory
    #[arg(short = 'w', long = "workdir")]
    pub remote_dir: Option<PathBuf>,

    /// Increase message verbosity (-v for debug, -vv for trace)
    #[arg(short, long, action = ArgAction::Count, global = true)]
    pub verbose: u8,
    /// Disable host-key warnings (unknown-host warning)
    #[arg(long, global = true)]
    pub no_warnings: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Analyze docker-compose configuration without deploying
    Analyze(analyze::Analyze),

    /// Deploy or update services
    Up(up::Up),

    /// Show service status
    Status(status::Status),

    /// Destroy deployment completely
    Destroy(destroy::Destroy),
}
