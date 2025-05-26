use super::error::CliError;
use super::parser::Cli;
use super::ui;
use crate::composer::{
    engine::Composer,
    errors::ComposerError,
    types::{ComposerConfig, ComposerOutput},
};
use crate::executor::LocalCommandExecutor;
use anyhow::Result;
use colored::Colorize;
use std::path::PathBuf;
use tabled::{
    settings::{object::Rows, Color, Modify, Style},
    Table, Tabled,
};
use tracing::info;
use url::Url;

// Helper struct to hold parsed SSH target details
#[derive(Debug)]
pub struct SshTarget {
    pub user: String,
    pub host: String,
    pub port: u16,
}

// Parsing function using the url crate
pub fn parse_ssh_target(target_str: &str) -> Result<SshTarget, CliError> {
    let default_user = "root";
    let default_port = 22;

    // Prepend scheme if missing to satisfy Url::parse requirements
    let parse_input =
        if !target_str.contains("://") && !target_str.contains('@') && !target_str.contains(':') {
            // Assume it's just a host if no user or port specified
            format!("ssh://{}", target_str)
        } else if !target_str.contains("://") {
            format!("ssh://{}", target_str)
        } else {
            target_str.to_string()
        };

    let url = Url::parse(&parse_input).map_err(|e| {
        CliError::ConfigError(format!("Invalid target format '{}': {}", target_str, e))
    })?;

    let user = if url.username().is_empty() {
        default_user.to_string()
    } else {
        url.username().to_string()
    };

    let host = url
        .host_str()
        .ok_or_else(|| CliError::ConfigError(format!("Missing host in target '{}'", target_str)))?
        .to_string();

    let port = url.port().unwrap_or(default_port);

    Ok(SshTarget { user, host, port })
}

// Helper to perform local analysis
pub async fn get_analysis(cli: &Cli) -> Result<ComposerOutput, ComposerError> {
    let executor = LocalCommandExecutor::new();
    let composer_config = ComposerConfig {
        project_dir: PathBuf::from("./"), // TODO: Consider making this configurable or smarter
        compose_files: cli.compose_files.clone(),
        env_files: cli.env_files.clone(),
    };

    let mut composer = Composer::try_new(executor, composer_config).await?;
    info!(
        "Using local {} version {}",
        composer.compose_command.command_string(),
        composer.compose_version
    );
    composer.analyze().await
}

#[derive(Tabled)]
struct EnvVarRow<'a> {
    #[tabled(rename = "Variable")]
    variable: String, // Use String to hold colored output
    #[tabled(rename = "Value")]
    value: &'a str,
}

#[derive(Tabled)]
struct PortRow<'a> {
    #[tabled(rename = "Target")]
    target: String, // Use String to hold colored output
    #[tabled(rename = "Published")]
    published: String, // Use String to hold colored output
    #[tabled(rename = "Protocol")]
    protocol: &'a str,
}

// Helper to print analysis results with enhanced formatting
pub fn print_analysis_results(analysis: &ComposerOutput) {
    println!(
        "\n{}",
        ui::format_header("Docker Compose Analysis Results:")
    );

    println!(
        "\n{}: {}",
        ui::format_header("Resolved Project Directory"),
        ui::format_highlight(&analysis.resolved_project_dir.display().to_string())
    );

    println!("\n{}", ui::format_header("Resolved Compose Files:"));
    if analysis.resolved_compose_files.is_empty() {
        println!(
            "  {}",
            ui::format_warning("(None - defaults might be used if found)")
        );
    } else {
        for file in &analysis.resolved_compose_files {
            println!("  - {}", ui::format_highlight(&file.display().to_string()));
        }
    }

    println!("\n{}", ui::format_header("Resolved Environment Files:"));
    if analysis.resolved_env_files.is_empty() {
        println!(
            "  {}",
            ui::format_warning("(None - default .env might be used if found)")
        );
    } else {
        for file in &analysis.resolved_env_files {
            println!("  - {}", ui::format_highlight(&file.display().to_string()));
        }
    }

    if !analysis.missing_env.is_empty() {
        println!(
            "\n{}",
            ui::format_header("Missing required environment variables:")
        );
        for var in &analysis.missing_env {
            println!("  - {}", ui::format_warning(var)); // Use warning color
        }
    }

    println!("\n{}", ui::format_header("Consumed environment variables:"));
    if analysis.consumed_env.is_empty() {
        println!("  {}", ui::format_warning("(None)"));
    } else {
        let data: Vec<_> = analysis
            .consumed_env
            .iter()
            .map(|(key, value)| EnvVarRow {
                variable: ui::format_highlight(key),
                value,
            })
            .collect();

        let mut table = Table::new(data);
        table
            .with(Style::blank())
            .with(Modify::new(Rows::first()).with(Color::FG_GREEN))
            .with(
                Modify::new(Rows::first())
                    .with(tabled::settings::Format::content(|s| s.bold().to_string())),
            ); // Apply bold
        println!("{}", table);
    }

    println!("\n{}", ui::format_header("Exposed ports:"));
    if analysis.exposed_ports.is_empty() {
        println!("  {}", ui::format_warning("(None)"));
    } else {
        let data: Vec<_> = analysis
            .exposed_ports
            .iter()
            .map(|port| PortRow {
                target: ui::format_highlight(&port.target.to_string()),
                published: ui::format_highlight(&port.published),
                protocol: port.protocol.as_deref().unwrap_or("tcp"),
            })
            .collect();

        let mut table = Table::new(data);
        table
            .with(Style::blank())
            .with(Modify::new(Rows::first()).with(Color::FG_CYAN))
            .with(
                Modify::new(Rows::first())
                    .with(tabled::settings::Format::content(|s| s.bold().to_string())),
            ); // Apply bold
        println!("{}", table);
    }

    println!(
        "\n{}",
        ui::format_header("Local references (files/directories needed):")
    );
    if analysis.local_references.is_empty() {
        println!("  {}", ui::format_warning("(None)"));
    } else {
        for reference in &analysis.local_references {
            println!(
                "  - {}",
                ui::format_highlight(&reference.display().to_string())
            );
        }
    }

    println!("\n{}", ui::format_header("Docker Compose Profiles:"));
    if analysis.available_profiles.is_empty() {
        println!("  {}", ui::format_warning("(None defined)"));
    } else {
        println!("  Available profiles:");
        for profile in &analysis.available_profiles {
            println!("    - {}", ui::format_highlight(profile));
        }
    }

    if !analysis.active_profiles.is_empty() {
        println!("  Active profiles:");
        for profile in &analysis.active_profiles {
            println!("    - {}", ui::format_highlight(profile));
        }
    } else if !analysis.available_profiles.is_empty() {
        println!(
            "  {}",
            ui::format_warning("No profiles currently active (set COMPOSE_PROFILES to activate)")
        );
    }
}
