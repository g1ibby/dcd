pub mod error;
pub mod local_executor;
pub mod ssh_executor;
pub mod traits;
pub mod types;

pub use error::ExecutorError;
pub use local_executor::LocalCommandExecutor;
pub use ssh_executor::SshCommandExecutor;
pub use traits::{CommandExecutor, FileTransfer};
pub use types::{CommandOutput, CommandResult, OutputError, OutputFormat, ProcessedOutput};
