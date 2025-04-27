use super::common::{get_analysis, parse_ssh_target};
use super::error::CliError;
use super::parser::Cli;
use super::ui;
use super::ui::handle_deployer_events;
use crate::deployer::types::DeployerEvent;
use crate::deployer::{types::DeploymentConfig, Deployer};
use crate::executor::SshCommandExecutor;
use clap::Args;
use dialoguer::Confirm;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info, instrument, warn};

#[derive(Debug, Args)]
pub struct Destroy {
    /// Remote target in the format [user@]host[:port]
    #[arg(required = true)]
    target: String,

    /// Force destruction without confirmation and remove volumes
    #[arg(long)]
    force: bool,

    /// Disable interactive progress spinner and show only logs
    #[arg(long)]
    pub no_progress: bool,
}

impl Destroy {
    #[instrument(name = "destroy", skip(self, cli_args), fields(target = %self.target))]
    pub async fn run(&self, cli_args: &Cli) -> Result<(), CliError> {
        let target = parse_ssh_target(&self.target)?;
        info!(
            "Destroying deployment on {}",
            ui::format_highlight(&self.target)
        );
        debug!(user = %target.user, port = %target.port, key = %cli_args.identity_file.display(), "SSH details");

        // --- Setup Progress Reporting ---
        let (progress_sender, ui_update_task_handle) = if !self.no_progress {
            let (sender, receiver) = mpsc::channel::<DeployerEvent>(32);
            // Use a more neutral initial message as confirmation happens first
            let pb = ui::create_spinner("Preparing destruction...");
            let ui_task = tokio::spawn(handle_deployer_events(receiver, pb.clone()));
            (Some(sender), Some((ui_task, pb)))
        } else {
            info!("Progress spinner disabled via --no-progress.");
            (None, None)
        };

        // --- Confirmation Prompt ---
        if !self.force {
            warn!(
                "This action will stop and remove containers, networks, and potentially volumes."
            );
            if !Confirm::new()
                .with_prompt(format!(
                    "Are you sure you want to destroy the deployment on {}?",
                    ui::format_highlight(&self.target)
                ))
                .interact()
                .map_err(|e| {
                    CliError::OperationFailed(format!("Failed to get confirmation: {}", e))
                })?
            {
                // If user cancels, ensure progress bar (if exists) is cleared
                info!("Destruction cancelled by user.");
                if let Some((_, pb)) = ui_update_task_handle {
                    pb.finish_and_clear(); // Clear spinner on cancellation
                }
                return Ok(());
            }
            // If proceeding, update spinner message if it exists
            if let Some((_, pb)) = &ui_update_task_handle {
                pb.set_message("Proceeding with destruction...");
            } else {
                info!("Proceeding with destruction..."); // Log if no spinner
            }
        } else {
            warn!("--force flag provided. Skipping confirmation and forcing volume removal.");
        }

        // --- Local Analysis (Minimal) ---
        // Needed to determine project context and potentially volumes to remove
        info!("Performing local analysis to determine project context..."); // Use info log
        let analysis = get_analysis(cli_args).await.map_err(|e| {
            if let Some((_, pb)) = ui_update_task_handle.as_ref() {
                // Borrow handle
                pb.finish_with_message("❌ Local analysis failed".to_string());
            }
            CliError::OperationFailed(format!("Local analysis failed: {}", e))
        })?;
        info!("Local analysis complete.");

        // --- SSH Connection ---
        info!("Connecting to {}...", ui::format_highlight(&target.host));
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

        // --- Destruction ---
        let deploy_config = DeploymentConfig {
            project_dir: analysis.resolved_project_dir.clone(),
            remote_dir: cli_args.remote_dir.clone(),
            compose_files: analysis.resolved_compose_files.clone(),
            env_files: analysis.resolved_env_files.clone(),
            // Pass other analysis results needed for potential volume cleanup etc.
            consumed_env: analysis.consumed_env,
            exposed_ports: analysis.exposed_ports,
            local_references: analysis
                .local_references
                .iter()
                .map(PathBuf::from)
                .collect(),
            volumes: analysis.volumes,
        };

        // Instantiate Deployer, passing the sender
        let mut deployer = Deployer::new(deploy_config, &mut executor, progress_sender);

        // Destroy deployment. Pass `force` to control volume removal.
        let remove_volumes = self.force; // Only remove volumes if --force is used
        let destroy_result = deployer
            .destroy(remove_volumes, self.force, self.force) // Assuming second self.force maps to remove_images for now
            .await;

        // Drop deployer to close channel
        drop(deployer);

        // Wait for UI task and handle final spinner state
        if let Some((ui_task, pb)) = ui_update_task_handle {
            if let Err(e) = ui_task.await {
                tracing::error!("UI update task failed: {}", e);
            }
            match &destroy_result {
                Ok(_) => pb.finish_with_message("✅ Destruction tasks finished."),
                Err(_) => pb.finish_with_message("❌ Destruction failed.".to_string()),
            }
        }

        // Handle the result after UI is done
        let status = destroy_result
            .map_err(|e| CliError::OperationFailed(format!("Destruction failed: {}", e)))?;

        info!(
            "{}",
            ui::format_success("Deployment destroyed successfully!")
        );
        if !status.message.is_empty() {
            info!("Details:\n{}", status.message.trim());
        }

        Ok(())
    }
}
