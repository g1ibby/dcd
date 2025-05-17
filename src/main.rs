use colored::*;
use dcd::cli::parser::Commands;
use std::process;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    let cli_args = dcd::cli::parse_args();

    let progress_likely_active = match &cli_args.command {
        Commands::Up(up_args) => !up_args.no_progress,
        Commands::Status(up_args) => !up_args.no_progress,
        Commands::Destroy(up_args) => !up_args.no_progress,
        _ => false,
    };

    // Setup tracing subscriber
    // If progress is likely active, hide INFO logs by default to keep output clean.
    // Otherwise, show INFO logs by default. Verbosity flags override this.
    let default_level = if progress_likely_active && cli_args.verbose == 0 {
        LevelFilter::INFO // Hide INFO when progress bar is active and no -v
    } else {
        // Show INFO by default, or DEBUG/TRACE if -v/-vv is set
        match cli_args.verbose {
            0 => LevelFilter::INFO,
            1 => LevelFilter::DEBUG,
            _ => LevelFilter::TRACE,
        }
    };

    let env_filter = EnvFilter::builder()
        .with_default_directive(default_level.into())
        .with_env_var("DCD_LOG")
        .from_env_lossy();

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .init();

    // Execute the command
    if let Err(e) = dcd::cli::run(cli_args).await {
        // Print user-facing error message clearly
        eprintln!("{}: {}", "Error".red().bold(), e);
        process::exit(1);
    }
}
