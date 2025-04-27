use super::common::{get_analysis, print_analysis_results};
use super::error::CliError;
use super::parser::Cli;
use clap::Args;
use tracing::info;

#[derive(Debug, Args)]
pub struct Analyze {}

impl Analyze {
    pub async fn run(&self, cli_args: &Cli) -> Result<(), CliError> {
        info!("Analyzing Docker Compose configuration...");

        let analysis = get_analysis(cli_args)
            .await
            .map_err(|e| CliError::OperationFailed(format!("Local analysis failed: {}", e)))?;

        print_analysis_results(&analysis);

        Ok(())
    }
}
