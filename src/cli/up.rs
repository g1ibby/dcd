use super::common::{get_analysis, parse_ssh_target, print_analysis_results};
use super::error::CliError;
use super::parser::Cli;
use super::ui;
use super::ui::handle_deployer_events;
use crate::deployer::{types::DeploymentConfig, Deployer};
use crate::executor::SshCommandExecutor;
use clap::Args;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info, instrument};

#[derive(Debug, Args)]
pub struct Up {
    /// Remote target in the format [user@]host[:port]
    #[arg(required = true)]
    target: String,

    /// Don't verify service health after deployment
    #[arg(long)]
    no_health_check: bool,

    /// Disable interactive progress spinner and show only logs
    #[arg(long)]
    pub no_progress: bool,
}

impl Up {
    #[instrument(name = "up", skip(self, cli_args), fields(target = %self.target))]
    pub async fn run(&self, cli_args: &Cli) -> Result<(), CliError> {
        let target = parse_ssh_target(&self.target)?;
        info!(
            "Deploying services to {}",
            ui::format_highlight(&self.target)
        );
        debug!(user = %target.user, port = %target.port, key = %cli_args.identity_file.display(), "SSH details");

        // --- Local Analysis ---
        let analysis_pb = ui::create_spinner("Performing local analysis...");
        let analysis = get_analysis(cli_args).await.map_err(|e| {
            analysis_pb.finish_and_clear(); // Clear spinner on error
            CliError::OperationFailed(format!("Local analysis failed: {}", e))
        })?;
        analysis_pb.finish_with_message("Local analysis complete.");
        print_analysis_results(&analysis); // Keep this direct output for now

        // --- SSH Connection ---
        let ssh_pb = ui::create_spinner(&format!(
            "Connecting to {}...",
            ui::format_highlight(&target.host)
        ));
        let addr_str = format!("{}:{}", target.host, target.port);
        let mut executor = SshCommandExecutor::connect(
            &cli_args.identity_file,
            &target.user,
            &addr_str,
            Duration::from_secs(30), // TODO: Make timeout configurable
            cli_args.no_warnings,
        )
        .await
        .map_err(|e| {
            ssh_pb.finish_and_clear();
            CliError::OperationFailed(format!("SSH connection failed: {}", e))
        })?;
        ssh_pb.finish_with_message(format!(
            "Connected to {}.",
            ui::format_highlight(&target.host)
        ));

        // --- Deployment ---
        let (progress_sender, ui_update_task_handle) = if !self.no_progress {
            // Create a channel for progress updates
            let (sender, receiver) = mpsc::channel::<crate::deployer::types::DeployerEvent>(32); // Buffer size 32
            let deploy_pb = ui::create_spinner("Initializing deployment..."); // Initial message

            // Spawn a task to listen for progress events and update the UI
            // Clone the ProgressBar for the task.
            let ui_task = tokio::spawn(handle_deployer_events(receiver, deploy_pb.clone()));

            // Return the sender and the task handle (wrapped in Some)
            (Some(sender), Some((ui_task, deploy_pb))) // Store pb too
        } else {
            info!("Progress spinner disabled via --no-progress.");
            // No progress UI needed
            (None, None)
        };
        let deploy_config = DeploymentConfig {
            project_dir: analysis.resolved_project_dir.clone(),
            remote_dir: cli_args.remote_dir.clone(),
            compose_files: analysis.resolved_compose_files.clone(),
            env_files: analysis.resolved_env_files.clone(),
            consumed_env: analysis.consumed_env,
            exposed_ports: analysis.exposed_ports,
            local_references: analysis
                .local_references
                .iter()
                .map(PathBuf::from)
                .collect(),
            volumes: analysis.volumes,
        };

        // Instantiate Deployer, passing the sender end of the channel
        let mut deployer = Deployer::new(deploy_config, &mut executor, progress_sender);

        // Deploy with progress reporting
        let deploy_result = deployer.deploy().await;

        // Drop the deployer to release the progress_sender
        // This will close the channel and allow the ui_update_task to complete
        drop(deployer);

        // Wait for UI task and handle final spinner state only if the task exists
        if let Some((ui_task, deploy_pb)) = ui_update_task_handle {
            // Wait for the UI update task to finish processing any remaining messages
            if let Err(e) = ui_task.await {
                tracing::error!("UI update task failed: {}", e); // Log if the UI task panics
            }

            // Handle the final result of deployment and finish the spinner
            match &deploy_result {
                Ok(_) => {
                    deploy_pb.finish_with_message("✅ Deployment tasks finished.");
                }
                Err(_) => {
                    deploy_pb.finish_with_message("❌ Deployment failed".to_string());
                    // Keep it concise
                }
            }
        }

        let status = match deploy_result {
            Ok(status) => status,
            Err(e) => {
                return Err(CliError::OperationFailed(format!(
                    "Deployment failed: {}",
                    e
                )));
            }
        };

        // --- Health Check ---
        if !status.services_healthy && !self.no_health_check {
            return Err(CliError::OperationFailed(
                ui::format_warning("Some services are not healthy after deployment.").to_string(),
            ));
        } else if status.services_healthy {
            info!("{}", ui::format_success("All services reported healthy."));
        } else if self.no_health_check {
            info!("{}", ui::format_warning("Skipped service health check."));
        }

        info!("{}", ui::format_success("Deployment successful!"));
        Ok(())
    }
}
