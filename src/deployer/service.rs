use super::{
    docker_manager::{DockerManager, HealthCheckResult, SshDockerManager},
    firewall::{PortConfig, Protocol, UfwManager},
    sync::{EnvFileManager, FileSync, SyncPlan},
    types::{
        ComposeExec, DeployError, DeployResult, DeployerEvent, DeploymentConfig, DeploymentStatus,
    },
    DCD_ENV_FILE,
};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct Deployer<'a> {
    config: DeploymentConfig,
    executor: &'a mut (dyn ComposeExec + Send),
    resolved_remote_dir: PathBuf,
    progress_sender: Option<mpsc::Sender<DeployerEvent>>,
}

impl<'a> Deployer<'a> {
    const HEALTH_CHECK_RETRIES: u32 = 5;
    const MAX_STARTING_ATTEMPTS: u32 = 15;
    const HEALTH_CHECK_DELAY: Duration = Duration::from_secs(10);

    pub fn new(
        config: DeploymentConfig,
        executor: &'a mut (dyn ComposeExec + Send),
        progress_sender: Option<mpsc::Sender<DeployerEvent>>,
    ) -> Self {
        // Determine the final remote directory path
        let resolved_remote_dir = match &config.remote_dir {
            Some(user_path) => {
                tracing::debug!(
                    "Using user-provided remote directory: {}",
                    user_path.display()
                );
                user_path.clone()
            }
            None => {
                // Extract project name from config.project_dir
                let project_name = config.project_dir.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| {
                        tracing::warn!("Could not determine project directory name from '{}', using 'default_project'", config.project_dir.display());
                        "default_project".to_string() // Fallback name
                    });
                let default_path = PathBuf::from(format!("/opt/{}", project_name));
                tracing::info!(
                    "No --workdir provided. Using default remote directory: {}",
                    default_path.display()
                );
                default_path
            }
        };

        tracing::debug!(
            "Deployer created with project_dir: '{}', resolved_remote_dir: '{}'",
            config.project_dir.display(),
            resolved_remote_dir.display()
        );
        Self {
            config,
            executor,
            resolved_remote_dir,
            progress_sender,
        }
    }

    /// Helper to send progress events if a sender exists.
    async fn send_event(&self, event: DeployerEvent) {
        if let Some(sender) = &self.progress_sender {
            if let Err(e) = sender.send(event.clone()).await {
                // Log error if sending fails (receiver likely dropped in CLI)
                tracing::warn!("Failed to send progress event: {}", e);
            }
        }
    }

    /// Main deployment method
    pub async fn deploy(&mut self) -> DeployResult<DeploymentStatus> {
        let mut status = DeploymentStatus::new();

        tracing::info!("🚀 Starting deployment process...");
        self.send_event(DeployerEvent::StepStarted(
            "Starting Deployment".to_string(),
        ))
        .await;

        // Step 1: Prepare environment
        tracing::info!("Step 1: Preparing remote environment...");
        self.send_event(DeployerEvent::StepStarted(
            "Preparing environment".to_string(),
        ))
        .await;
        if let Err(e) = self.prepare_environment(&mut status).await {
            self.send_event(DeployerEvent::StepFailed(
                "Preparing environment".to_string(),
                e.to_string(),
            ))
            .await;
            return Err(e);
        }
        self.send_event(DeployerEvent::StepCompleted(
            "Preparing environment".to_string(),
        ))
        .await;

        // Step 2: Sync files
        tracing::info!("Step 2: Synchronizing project files...");
        self.send_event(DeployerEvent::StepStarted(
            "Synchronizing files".to_string(),
        ))
        .await;
        if let Err(e) = self.sync_files(&mut status).await {
            self.send_event(DeployerEvent::StepFailed(
                "Synchronizing files".to_string(),
                e.to_string(),
            ))
            .await;
            return Err(e);
        }
        self.send_event(DeployerEvent::StepCompleted(
            "Synchronizing files".to_string(),
        ))
        .await;

        // Step 3: Configure firewall
        tracing::info!("Step 3: Configuring firewall (UFW)...");
        self.send_event(DeployerEvent::StepStarted(
            "Configuring firewall".to_string(),
        ))
        .await;
        if let Err(e) = self.configure_firewall(&mut status).await {
            self.send_event(DeployerEvent::StepFailed(
                "Configuring firewall".to_string(),
                e.to_string(),
            ))
            .await;
            return Err(e);
        }
        self.send_event(DeployerEvent::StepCompleted(
            "Configuring firewall".to_string(),
        ))
        .await;

        // Step 4: Deploy services
        tracing::info!("Step 4: Deploying services using Docker Compose...");
        self.send_event(DeployerEvent::StepStarted("Deploying services".to_string()))
            .await;
        if let Err(e) = self.deploy_services(&mut status).await {
            self.send_event(DeployerEvent::StepFailed(
                "Deploying services".to_string(),
                e.to_string(),
            ))
            .await;
            return Err(e);
        }
        self.send_event(DeployerEvent::StepCompleted(
            "Deploying services".to_string(),
        ))
        .await;

        Ok(status)
    }

    pub async fn destroy(
        &mut self,
        remove_volumes: bool,
        remove_images: bool,
        force: bool,
    ) -> DeployResult<DeploymentStatus> {
        let mut status = DeploymentStatus::new();
        self.send_event(DeployerEvent::StepStarted(
            "Initializing destruction...".to_string(),
        ))
        .await;
        tracing::info!("🔥 Starting destruction process...");

        // Clone the sender before creating the manager which borrows self.executor mutably
        let cloned_sender = self.progress_sender.clone();

        // Create Docker manager
        tracing::debug!("Initializing Docker manager for destruction.");
        // Build list of remote compose and env files (basenames)
        let compose_files = self
            .config
            .compose_files
            .iter()
            .map(|p| PathBuf::from(p.file_name().expect("Invalid compose file path")))
            .collect::<Vec<PathBuf>>();
        let mut env_files = self
            .config
            .env_files
            .iter()
            .map(|p| PathBuf::from(p.file_name().expect("Invalid env file path")))
            .collect::<Vec<PathBuf>>();
        // Include generated .env.dcd if present
        let dcd_path = self.config.project_dir.join(DCD_ENV_FILE);
        if dcd_path.exists() {
            env_files.push(PathBuf::from(DCD_ENV_FILE));
        }
        let mut docker_manager = SshDockerManager::new(
            self.executor,
            self.resolved_remote_dir.clone(),
            compose_files,
            env_files,
        )
        .await?;

        // Check if any services are running
        tracing::info!("Checking for running services...");
        let services_running = docker_manager.has_running_services().await?;

        // If services are running and force is false, return error
        if services_running && !force {
            tracing::warn!("Services are running. Destruction aborted. Use --force to override.");
            status.message = "Services are still running. Use --force to destroy anyway.".into();
            return Err(DeployError::Deployment(status.message.clone()));
        } else if services_running && force {
            tracing::info!(
                "Force flag enabled. Proceeding with destruction despite running services."
            );
        }

        // Stop and remove containers
        tracing::info!(
            "Stopping services, removing containers{}{}...",
            if remove_volumes { ", volumes" } else { "" },
            if remove_images { ", images" } else { "" }
        );
        // Use the cloned sender
        if let Some(sender) = &cloned_sender {
            let _ = sender
                .send(DeployerEvent::StepStarted(
                    "Stopping and removing containers/networks...".to_string(),
                ))
                .await;
        }
        docker_manager
            .compose_down(remove_volumes, remove_images)
            .await?;
        if let Some(sender) = &cloned_sender {
            let _ = sender
                .send(DeployerEvent::StepCompleted(
                    "Containers and networks removed.".to_string(),
                ))
                .await;
        }

        let mut removal_details = Vec::new();
        if remove_volumes {
            removal_details.push("volumes");
        }
        if remove_images {
            removal_details.push("images");
        }

        // If removing volumes, also remove the remote project directory
        if remove_volumes {
            let remote_dir_str = self.resolved_remote_dir.display().to_string();
            // Use the cloned sender
            if let Some(sender) = &cloned_sender {
                let _ = sender
                    .send(DeployerEvent::StepStarted(format!(
                        "Removing remote directory: {}",
                        remote_dir_str
                    )))
                    .await;
            }
            tracing::info!("Removing remote project directory: {}", remote_dir_str);
            let rm_cmd = format!("rm -rf {}", remote_dir_str);
            let rm_result = self.executor.execute_command(&rm_cmd).await.map_err(|e| {
                DeployError::Deployment(format!("Failed to remove directory: {}", e))
            })?;
            if !rm_result.is_success() {
                let error_msg = rm_result.output.to_stderr_string()?;
                tracing::error!(
                    "Failed to remove remote directory '{}': {}",
                    remote_dir_str,
                    error_msg
                );
                // Log error but don't fail the whole destroy operation, as compose down succeeded
                status.message = format!("Deployment destroyed (containers{}, images{}), but failed to remove project directory: {}", 
                    if remove_volumes {"+volumes"} else {""}, 
                    if remove_images {"+images"} else {""}, 
                    error_msg);
            } else {
                removal_details.push("project directory");
                // Use the cloned sender
                if let Some(sender) = &cloned_sender {
                    let _ = sender
                        .send(DeployerEvent::StepCompleted(
                            "Remote directory removed.".to_string(),
                        ))
                        .await;
                }
            }
        }

        status.message = format!(
            "Deployment destroyed successfully (removed: {}).",
            if removal_details.is_empty() {
                "containers only".to_string()
            } else {
                removal_details.join(", ")
            }
        );

        self.send_event(DeployerEvent::StepCompleted(
            "Destruction complete.".to_string(),
        ))
        .await;
        Ok(status)
    }

    /// Prepare environment (env files, directories)
    async fn prepare_environment(&mut self, status: &mut DeploymentStatus) -> DeployResult<()> {
        tracing::debug!("Initializing environment file manager.");
        // Create env file manager
        let env_manager =
            EnvFileManager::new(self.config.consumed_env.clone(), &self.config.project_dir);

        // Generate .env.dcd if we have consumed environment variables
        if env_manager.has_env_vars() {
            tracing::info!("Generating {} file locally...", DCD_ENV_FILE);
            env_manager.generate_dcd_env().await?;
            tracing::debug!(
                "{} generated at: {}",
                DCD_ENV_FILE,
                env_manager.get_dcd_env_path().display()
            );
            status.env_changed = true;
        } else {
            tracing::debug!(
                "No consumed environment variables, skipping {} generation.",
                DCD_ENV_FILE
            );
        }

        Ok(())
    }

    /// Synchronize all required files
    async fn sync_files(&mut self, status: &mut DeploymentStatus) -> DeployResult<()> {
        let mut sync_plan = SyncPlan::new();
        tracing::debug!("Initializing file synchronization plan.");
        // Keep track of top-level project directories/files already added to the plan
        // to avoid duplicates when multiple files from the same dir are referenced.
        let mut synced_top_level_paths: HashSet<PathBuf> = HashSet::new();

        // Add compose files
        for file in &self.config.compose_files {
            let remote_path =
                self.resolved_remote_dir
                    .join(file.file_name().ok_or_else(|| {
                        DeployError::Configuration("Invalid compose file name".into())
                    })?);
            tracing::debug!(
                "Adding compose file to sync plan: '{}' -> '{}'",
                file.display(),
                remote_path.display()
            );
            sync_plan.add_compose_file(file, remote_path);
        }

        // Add env files
        for file in &self.config.env_files {
            let remote_path = self.resolved_remote_dir.join(
                file.file_name()
                    .ok_or_else(|| DeployError::Configuration("Invalid env file name".into()))?,
            );
            tracing::debug!(
                "Adding env file to sync plan: '{}' -> '{}'",
                file.display(),
                remote_path.display()
            );
            sync_plan.add_env_file(file, remote_path);
        }

        // Add .env.dcd if it exists
        let dcd_env = self.config.project_dir.join(DCD_ENV_FILE);
        if dcd_env.exists() {
            tracing::debug!(
                "Adding generated {} to sync plan: '{}' -> '{}'",
                DCD_ENV_FILE,
                dcd_env.display(),
                self.resolved_remote_dir.join(DCD_ENV_FILE).display()
            );
            sync_plan.add_env_file(&dcd_env, self.resolved_remote_dir.join(DCD_ENV_FILE));
        }

        // Add referenced files
        for path in &self.config.local_references {
            tracing::debug!("Processing local reference: '{}'", path.display());
            // Try to make the path relative to the resolved project directory
            match path.strip_prefix(&self.config.project_dir) {
                Ok(relative_path) => {
                    // Path is inside the project. Check if it exists locally.
                    if !path.exists() {
                        tracing::warn!(
                            "Local reference path inside project directory does not exist, skipping: {}",
                            path.display()
                        );
                        continue;
                    }

                    // Get the first component of the relative path (e.g., "traefik" from "traefik/config/file.yml")
                    if let Some(first_component) = relative_path.components().next() {
                        let top_level_component = first_component.as_os_str();
                        let top_level_path = self.config.project_dir.join(top_level_component);
                        let remote_top_level_path =
                            self.resolved_remote_dir.join(top_level_component);
                        tracing::debug!(
                            "Identified top-level component '{}' for reference '{}'",
                            top_level_component.to_string_lossy(),
                            path.display()
                        );

                        // Only add if we haven't added this top-level path already
                        if synced_top_level_paths.insert(top_level_path.clone()) {
                            // Add the top-level directory containing the reference to the sync plan.
                            // Always sync as a directory based on the user request.
                            tracing::debug!(
                                "Adding reference directory to sync plan: '{}' -> '{}'",
                                top_level_path.display(),
                                remote_top_level_path.display()
                            );
                            sync_plan.add_reference(&top_level_path, remote_top_level_path, true);
                        }
                    } // else: relative_path was empty or unusual, ignore.
                }
                Err(_) => {
                    // Path is outside the project directory, log and skip syncing
                    // We don't need to check path.exists() for external paths.
                    tracing::info!(
                        "External reference '{}' found, will not be synced by dcd.",
                        path.display()
                    );
                }
            }
        }

        // Perform synchronization
        tracing::info!("Executing file synchronization...");
        let mut file_sync = FileSync::new(self.executor, self.resolved_remote_dir.clone());
        let sync_status = file_sync.sync_files(&sync_plan).await?;

        // Update deployment status
        status.files_changed = !sync_status.files_synced.is_empty();
        tracing::debug!(
            "Sync results: {} files synced, {} skipped, {} failed.",
            sync_status.files_synced.len(),
            sync_status.files_skipped.len(),
            sync_status.files_failed.len()
        );

        if !sync_status.files_failed.is_empty() {
            let failed_files: Vec<_> = sync_status
                .files_failed
                .iter()
                .map(|(path, _)| path.display().to_string())
                .collect();
            status.message = format!("Failed to sync files: {}", failed_files.join(", "));
            tracing::error!(
                "File synchronization failed for: {}",
                failed_files.join(", ")
            );
            return Err(DeployError::FileSync(status.message.clone()));
        }

        Ok(())
    }

    /// Configure firewall rules
    async fn configure_firewall(&mut self, status: &mut DeploymentStatus) -> DeployResult<()> {
        if self.config.exposed_ports.is_empty() {
            tracing::info!("No exposed ports found in configuration, skipping firewall setup.");
            return Ok(());
        }

        tracing::debug!("Initializing UFW manager.");
        let mut ufw = UfwManager::new(self.executor);

        // Convert exposed ports to firewall config
        let port_configs: Vec<PortConfig> = self
            .config
            .exposed_ports
            .iter()
            .map(|port| PortConfig {
                port: port.target,
                protocol: Protocol::from(port.protocol.as_deref().unwrap_or("tcp")),
                description: format!("Docker service port {}", port.published),
            })
            .collect();

        tracing::info!(
            "Applying firewall rules for {} port(s)...",
            port_configs.len()
        );
        tracing::debug!("Port configurations to apply: {:?}", port_configs);
        // Configure ports
        ufw.configure_ports(&port_configs).await?;

        // TODO: check why i don't pass this check
        // Verify port accessibility
        tracing::info!("Verifying firewall rules...");
        for config in &port_configs {
            tracing::debug!("Verifying port {}/{}", config.port, config.protocol);
            if !ufw.verify_port(config.port, &config.protocol).await? {
                status.message = format!("Port {} is not accessible", config.port);
                status.ports_changed = true;
                tracing::warn!(
                    "Verification failed: Port {}/{} is not accessible after configuration.",
                    config.port,
                    config.protocol
                );
                // Decide if this should be a hard error or just a warning in status
            }
        }

        Ok(())
    }

    /// Deploy services using docker-compose
    async fn deploy_services(&mut self, status: &mut DeploymentStatus) -> DeployResult<()> {
        tracing::debug!("Initializing Docker manager for service deployment.");
        // Build list of remote compose and env files (basenames)
        let compose_files = self
            .config
            .compose_files
            .iter()
            .map(|p| PathBuf::from(p.file_name().expect("Invalid compose file path")))
            .collect::<Vec<PathBuf>>();
        let mut env_files = self
            .config
            .env_files
            .iter()
            .map(|p| PathBuf::from(p.file_name().expect("Invalid env file path")))
            .collect::<Vec<PathBuf>>();
        // Include generated .env.dcd if present
        let dcd_path = self.config.project_dir.join(DCD_ENV_FILE);
        if dcd_path.exists() {
            env_files.push(PathBuf::from(DCD_ENV_FILE));
        }
        let mut docker_manager = SshDockerManager::new(
            self.executor,
            self.resolved_remote_dir.clone(),
            compose_files,
            env_files,
        )
        .await?;

        tracing::info!("Ensuring Docker is installed on remote host...");
        docker_manager
            .ensure_docker_installed()
            .await
            .map_err(|e| {
                tracing::error!("Docker installation check failed: {}", e);
                e
            })?;

        tracing::info!("Ensuring Docker Compose is installed and compatible...");
        docker_manager
            .ensure_docker_compose_installed()
            .await
            .map_err(|e| {
                tracing::error!("Docker Compose installation check failed: {}", e);
                e
            })?;

        // Start services
        tracing::info!("Running 'docker compose up -d' ...");
        docker_manager.compose_up().await?;

        // Clone the sender before the loop to avoid borrow conflicts with docker_manager
        let cloned_sender = self.progress_sender.clone();
        // Helper closure: Captures cloned_sender by reference.
        // The async move block takes ownership of 'event' and the reference to cloned_sender.
        let send_event_local = |event: DeployerEvent| {
            // Capture by reference here
            let sender_ref = &cloned_sender;
            async move {
                // Move 'event' and 'sender_ref'
                if let Some(sender) = sender_ref.as_ref() {
                    // Use Option::as_ref() to get Option<&Sender>
                    if let Err(e) = sender.send(event).await {
                        // Send the moved event using &Sender
                        tracing::warn!("Failed to send progress event (local): {}", e);
                    }
                }
            }
        };

        tracing::info!("Checking health of deployed services...");
        let mut attempts = 0;

        loop {
            attempts += 1;
            tracing::info!(
                "Health check attempt {}/{}...",
                attempts,
                Self::HEALTH_CHECK_RETRIES
            );
            send_event_local(DeployerEvent::HealthCheckAttempt(
                attempts,
                Self::HEALTH_CHECK_RETRIES,
            ))
            .await;

            match docker_manager.verify_services_healthy().await {
                Ok(HealthCheckResult::Healthy) => {
                    tracing::info!("✅ Services are healthy.");
                    send_event_local(DeployerEvent::HealthCheckStatus(
                        "All services are healthy".to_string(),
                    ))
                    .await;
                    status.services_healthy = true;
                    break;
                }
                Ok(HealthCheckResult::NoServices) => {
                    tracing::warn!("Health check: No services found.");
                    send_event_local(DeployerEvent::HealthCheckStatus(
                        "No services found".to_string(),
                    ))
                    .await;
                    status.services_healthy = false;
                    status.message = "No services were found after deployment.".into();
                    break;
                }
                Ok(HealthCheckResult::Failed(failed_services))
                    if attempts < Self::HEALTH_CHECK_RETRIES =>
                {
                    tracing::warn!(
                        "Services not healthy yet. Found {} unhealthy service(s). Retrying in {:?}...", 
                        failed_services.len(),
                        Self::HEALTH_CHECK_DELAY
                    );
                    send_event_local(DeployerEvent::HealthCheckStatus(format!(
                        "{} unhealthy service(s), retrying...",
                        failed_services.len()
                    )))
                    .await;
                    tokio::time::sleep(Self::HEALTH_CHECK_DELAY).await;
                }
                Ok(HealthCheckResult::Failed(failed_services)) => {
                    // Final attempt with unhealthy services
                    status.services_healthy = false;

                    // Create a detailed message about unhealthy services
                    let service_details: Vec<String> = failed_services
                        .iter()
                        .map(|s| {
                            format!(
                                "{} (state: {}, health: {})",
                                s.name,
                                s.state,
                                if s.health.is_empty() {
                                    "no health check"
                                } else {
                                    &s.health
                                }
                            )
                        })
                        .collect();

                    status.message = format!(
                        "Definitively unhealthy services found after {} attempts: {}.",
                        attempts,
                        service_details.join("; ")
                    );

                    let event_msg = format!(
                        "Health check failed: {} unhealthy service(s)",
                        failed_services.len()
                    );
                    send_event_local(DeployerEvent::HealthCheckStatus(event_msg)).await;

                    tracing::error!(
                        "❌ Health check failed (terminal state): {}",
                        status.message
                    );
                    break;
                }
                Ok(HealthCheckResult::Starting(starting_services))
                    if attempts < Self::MAX_STARTING_ATTEMPTS =>
                {
                    tracing::info!(
                        "Health check: {} service(s) still starting. Waiting longer (attempt {}/{})...",
                        starting_services.len(),
                        attempts,
                        Self::MAX_STARTING_ATTEMPTS
                    );
                    send_event_local(DeployerEvent::HealthCheckStatus(format!(
                        "{} service(s) still starting...",
                        starting_services.len()
                    )))
                    .await;
                    tokio::time::sleep(Self::HEALTH_CHECK_DELAY).await;
                }
                Ok(HealthCheckResult::Starting(starting_services)) => {
                    // Max starting attempts reached
                    status.services_healthy = false;
                    let service_details: Vec<String> = starting_services
                        .iter()
                        .map(|s| format!("{} (state: {}, health: {})", s.name, s.state, s.health))
                        .collect();

                    status.message = format!(
                        "Services still in 'starting' state after extended timeout ({} attempts): {}.",
                        attempts,
                        service_details.join("; ")
                    );

                    send_event_local(DeployerEvent::HealthCheckStatus(format!(
                        "Timeout: {} service(s) still starting",
                        starting_services.len()
                    )))
                    .await;

                    tracing::error!("❌ Health check failed: {}", status.message);
                    break;
                }
                Err(e) => {
                    status.services_healthy = false;
                    status.message = format!("Error checking service health: {}", e);
                    send_event_local(DeployerEvent::HealthCheckStatus(
                        "Error checking service health".to_string(),
                    ))
                    .await;
                    tracing::error!("❌ Health check error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Get current deployment status
    pub async fn get_status(&mut self) -> DeployResult<DeploymentStatus> {
        tracing::info!("Checking current deployment status...");
        self.send_event(DeployerEvent::StepStarted(
            "Initializing status check...".to_string(),
        ))
        .await;

        // Clone the sender before creating the manager which borrows self.executor mutably
        let cloned_sender = self.progress_sender.clone();

        tracing::debug!("Initializing Docker manager for status check.");
        // Use the cloned sender
        if let Some(sender) = &cloned_sender {
            let _ = sender
                .send(DeployerEvent::StepStarted(
                    "Connecting to Docker...".to_string(),
                ))
                .await;
        }
        // Build list of remote compose and env files (basenames)
        let compose_files = self
            .config
            .compose_files
            .iter()
            .map(|p| PathBuf::from(p.file_name().expect("Invalid compose file path")))
            .collect::<Vec<PathBuf>>();
        let env_files = self
            .config
            .env_files
            .iter()
            .map(|p| PathBuf::from(p.file_name().expect("Invalid env file path")))
            .collect::<Vec<PathBuf>>();
        let mut docker_manager = SshDockerManager::new(
            self.executor,
            self.resolved_remote_dir.clone(),
            compose_files,
            env_files,
        )
        .await?;
        if let Some(sender) = &cloned_sender {
            let _ = sender
                .send(DeployerEvent::StepCompleted(
                    "Connected to Docker.".to_string(),
                ))
                .await;
        }

        let mut status = DeploymentStatus::new();

        // Check services health
        tracing::info!("Checking service health...");
        // Use the cloned sender
        if let Some(sender) = &cloned_sender {
            let _ = sender
                .send(DeployerEvent::StepStarted(
                    "Checking service health...".to_string(),
                ))
                .await;
        }
        match docker_manager.verify_services_healthy().await? {
            HealthCheckResult::Healthy => {
                status.services_healthy = true;
                tracing::info!("Service health status: Healthy");
            }
            HealthCheckResult::NoServices => {
                status.services_healthy = false;
                status.message = "No services found.".into();
                tracing::warn!("Service health status: No services found");
            }
            HealthCheckResult::Failed(failed_services) => {
                status.services_healthy = false;

                // Create a detailed message about unhealthy services
                let service_details: Vec<String> = failed_services
                    .iter()
                    .map(|s| {
                        format!(
                            "{} (state: {}, health: {})",
                            s.name,
                            s.state,
                            if s.health.is_empty() {
                                "no health check"
                            } else {
                                &s.health
                            }
                        )
                    })
                    .collect();

                status.message = format!("Failed services: {}.", service_details.join("; "));

                tracing::warn!("Service health status: Failed - {}", status.message);
            }
            HealthCheckResult::Starting(starting_services) => {
                status.services_healthy = false;

                let service_details: Vec<String> = starting_services
                    .iter()
                    .map(|s| format!("{} (state: {}, health: {})", s.name, s.state, s.health))
                    .collect();

                status.message =
                    format!("Services still starting: {}.", service_details.join("; "));

                tracing::info!("Service health status: Starting - {}", status.message);
            }
        }
        // Use the cloned sender for the final event within the scope of docker_manager
        if let Some(sender) = &cloned_sender {
            let _ = sender
                .send(DeployerEvent::StepCompleted(
                    "Service health checked.".to_string(),
                ))
                .await;
        }

        Ok(status)
    }
}
