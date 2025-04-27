use super::common::{get_analysis, parse_ssh_target};
use super::error::CliError;
use super::parser::Cli;
use super::ui;
use super::ui::handle_deployer_events;
use crate::deployer::types::DeployerEvent;
use crate::deployer::{types::DeploymentConfig, Deployer};
use crate::executor::SshCommandExecutor;
use clap::Args;
use colored::*;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info, instrument};

#[derive(Debug, Args)]
pub struct Status {
    /// Remote target in the format [user@]host[:port]
    #[arg(required = true)]
    target: String,

    /// Disable interactive progress spinner and show only logs
    #[arg(long)]
    pub no_progress: bool,
}

impl Status {
    #[instrument(name = "status", skip(self, cli_args), fields(target = %self.target))]
    pub async fn run(&self, cli_args: &Cli) -> Result<(), CliError> {
        let target = parse_ssh_target(&self.target)?;
        info!("Checking status on {}", ui::format_highlight(&self.target));
        debug!(user = %target.user, port = %target.port, key = %cli_args.identity_file.display(), "SSH details");

        // --- Setup Progress Reporting ---
        let (progress_sender, ui_update_task_handle) = if !self.no_progress {
            let (sender, receiver) = mpsc::channel::<DeployerEvent>(32);
            let pb = ui::create_spinner("Initializing status check..."); // Initial message
            let ui_task = tokio::spawn(handle_deployer_events(receiver, pb.clone()));
            (Some(sender), Some((ui_task, pb)))
        } else {
            info!("Progress spinner disabled via --no-progress.");
            (None, None)
        };

        // --- Local Analysis (Minimal) ---
        info!("Performing local analysis to determine project context..."); // Use info log
        let analysis = get_analysis(cli_args).await.map_err(|e| {
            // If progress bar exists, finish it with error before returning
            if let Some((_, pb)) = ui_update_task_handle.as_ref() {
                // Borrow handle
                pb.finish_with_message("❌ Local analysis failed".to_string());
            }
            CliError::OperationFailed(format!("Local analysis failed: {}", e))
        })?;
        info!("Local analysis complete."); // Use info log

        // --- SSH Connection ---
        info!("Connecting to {}...", ui::format_highlight(&target.host)); // Use info log
        let addr_str = format!("{}:{}", target.host, target.port);
        let mut executor = SshCommandExecutor::connect(
            &cli_args.identity_file,
            &target.user,
            &addr_str,
            Duration::from_secs(30),
        )
        .await
        .map_err(|e| {
            if let Some((_, pb)) = ui_update_task_handle.as_ref() {
                // Borrow handle
                pb.finish_with_message("❌ SSH connection failed".to_string());
            }
            CliError::OperationFailed(format!("SSH connection failed: {}", e))
        })?;
        info!("Connected to {}.", ui::format_highlight(&target.host));

        // --- Get Status ---
        // Create deployment config with minimal required information for status check
        let deploy_config = DeploymentConfig {
            project_dir: analysis.resolved_project_dir.clone(),
            remote_dir: cli_args.remote_dir.clone(),
            compose_files: analysis.resolved_compose_files.clone(),
            env_files: analysis.resolved_env_files.clone(),
            // Following fields are not strictly needed for basic status but required by struct
            consumed_env: std::collections::HashMap::new(),
            exposed_ports: Vec::new(),
            local_references: Vec::new(),
            volumes: Vec::new(),
        };

        // Instantiate Deployer, passing the sender
        let mut deployer = Deployer::new(deploy_config, &mut executor, progress_sender);

        // Get status
        let status_result = deployer.get_status().await;

        // Drop deployer to close channel
        drop(deployer);

        // Wait for UI task and handle final spinner state
        if let Some((ui_task, pb)) = ui_update_task_handle {
            if let Err(e) = ui_task.await {
                tracing::error!("UI update task failed: {}", e);
            }
            match &status_result {
                Ok(_) => pb.finish_with_message("✅ Status check finished."),
                Err(_) => pb.finish_with_message("❌ Status check failed.".to_string()),
            }
        }

        // Handle the result after UI is done
        let status = status_result
            .map_err(|e| CliError::OperationFailed(format!("Status check failed: {}", e)))?;

        // --- Print Status ---
        println!(
            "\n{}",
            ui::format_header(&format!("Deployment Status on {}:", self.target))
        );
        let health_status = if status.services_healthy {
            "Yes".green()
        } else {
            "No".red()
        };
        println!("Services healthy: {}", health_status);

        if !status.message.is_empty() {
            println!("Status message:\n{}", status.message.trim());
        } else {
            println!("(No detailed status message provided by docker compose ps)");
        }

        Ok(())
    }
}
