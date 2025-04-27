use crate::deployer::types::DeployerEvent;
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;
use tokio::sync::mpsc;

/// Creates a standard spinner ProgressBar.
pub fn create_spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner:.blue.bold} {msg}")
            .unwrap()
            // More templates: https://docs.rs/indicatif/#templates
            .tick_strings(&[
                "▹▹▹▹▹",
                "▸▹▹▹▹",
                "▹▸▹▹▹",
                "▹▹▸▹▹",
                "▹▹▹▸▹",
                "▹▹▹▹▸",
                "▪▪▪▪▪",
            ]),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(120));
    pb
}

/// Handles receiving DeployerEvents and updating a ProgressBar.
pub async fn handle_deployer_events(mut receiver: mpsc::Receiver<DeployerEvent>, pb: ProgressBar) {
    while let Some(event) = receiver.recv().await {
        match event {
            DeployerEvent::StepStarted(msg) => {
                pb.set_message(format!("⏳ {}", msg));
            }
            DeployerEvent::StepCompleted(msg) => {
                pb.set_message(format!("✅ {}", msg));
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            DeployerEvent::StepFailed(step, err) => {
                pb.set_message(format!("❌ Error during {}: {}", step, err));
                break;
            }
            DeployerEvent::HealthCheckAttempt(attempt, total) => {
                pb.set_message(format!("🩺 Health check ({}/{})", attempt, total));
            }
            DeployerEvent::HealthCheckStatus(status_msg) => {
                pb.set_message(format!("🩺 {}", status_msg));
            }
        }
    }
}

pub fn format_header(text: &str) -> String {
    format!("{}", text.blue().bold())
}

pub fn format_highlight(text: &str) -> String {
    format!("{}", text.cyan())
}

pub fn format_success(text: &str) -> String {
    format!("{}", text.green())
}

pub fn format_warning(text: &str) -> String {
    format!("{}", text.yellow())
}
