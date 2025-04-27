mod analyze;
mod common;
mod destroy;
mod error;
pub mod parser;
mod status;
mod ui;
mod up;

use clap::Parser;
use error::CliError;
use parser::Cli;

// Helper function to parse args
pub fn parse_args() -> Cli {
    Cli::parse()
}

// Main CLI execution function, receives parsed args
pub async fn run(cli: Cli) -> Result<(), CliError> {
    // Match the command and call its specific run method
    match &cli.command {
        parser::Commands::Analyze(cmd) => cmd.run(&cli).await,
        parser::Commands::Up(cmd) => cmd.run(&cli).await,
        parser::Commands::Status(cmd) => cmd.run(&cli).await,
        parser::Commands::Destroy(cmd) => cmd.run(&cli).await,
    }
}
