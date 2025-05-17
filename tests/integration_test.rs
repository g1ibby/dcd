use std::time::Duration;
use tempfile::TempDir;
use testcontainers::{
    core::{IntoContainerPort, Mount},
    runners::AsyncRunner,
    ImageExt,
};
use tokio::fs;
use tokio::process::Command;
use tokio::time::sleep;

mod common;
use common::{ssh_cmd, SshServer};

#[tokio::test]
async fn test_ssh_server() {
    // Start SSH server container
    let container = SshServer
        .with_mapped_port(0, 22.tcp())
        .with_mount(Mount::bind_mount(
            "/var/run/docker.sock",
            "/var/run/docker.sock",
        ))
        .start()
        .await
        .expect("failed to start SSH container");

    // Verify SSH port is mapped
    let host_port = container
        .get_host_port_ipv4(22)
        .await
        .expect("SSH port not mapped");
    assert!(
        host_port > 0,
        "SSH server port mapping should be a positive port"
    );

    // Try SSH connection with key-based authentication
    let status = ssh_cmd(
        host_port,
        "tests/test_ssh_key",
        "root@127.0.0.1",
        &["echo", "hello"],
    )
    .status()
    .await
    .expect("failed to execute ssh command");
    assert!(status.success(), "SSH command failed");

    // Verify output
    let output = ssh_cmd(
        host_port,
        "tests/test_ssh_key",
        "root@127.0.0.1",
        &["echo", "hello"],
    )
    .output()
    .await
    .expect("failed to execute ssh command");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "hello");

    // Stop the container
    container.stop().await.unwrap();
}

#[tokio::test]
async fn test_dcd_up_deploy_nginx() {
    // Start SSH server container
    let container = SshServer
        .with_mapped_port(0, 22.tcp())
        .with_mount(Mount::bind_mount(
            "/var/run/docker.sock",
            "/var/run/docker.sock",
        ))
        .start()
        .await
        .expect("failed to start SSH container");
    let ssh_port = container
        .get_host_port_ipv4(22)
        .await
        .expect("SSH port not mapped");

    // Prepare temporary project directory with a simple nginx compose file
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_dir = temp_dir.path();
    let compose_path = project_dir.join("docker-compose.yml");
    let compose_content = [
        "version: '3'",
        "services:",
        "  nginx:",
        "    image: nginx:alpine",
        "    ports:",
        "      - \"8080:80\"",
    ]
    .join("\n");
    fs::write(&compose_path, compose_content)
        .await
        .expect("Failed to write docker-compose.yml");

    // Create an empty .env file
    let env_path = project_dir.join(".env");
    fs::write(&env_path, "")
        .await
        .expect("Failed to write .env file");

    // Run the DCD up command to deploy nginx
    let target = format!("root@localhost:{}", ssh_port);
    let output = Command::new("cargo")
        .args([
            "run",
            "-q",
            "--",
            "-f",
            compose_path.to_str().unwrap(),
            "-e",
            env_path.to_str().unwrap(),
            "-i",
            "tests/test_ssh_key",
            "-w",
            "/opt/test-project",
            "up",
            &target,
        ])
        .output()
        .await
        .expect("Failed to execute DCD up command");
    assert!(
        output.status.success(),
        "DCD up command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The success message may be logged to stderr (via tracing) or stdout
    assert!(
        stdout.contains("Deployment successful") || stderr.contains("Deployment successful"),
        "Unexpected output:\n--- STDOUT ---\n{}\n--- STDERR ---\n{}",
        stdout,
        stderr
    );

    // Allow some time for Docker Compose to start containers
    sleep(Duration::from_secs(5)).await;

    // Verify nginx container is running via SSH
    let ps_output = ssh_cmd(
        ssh_port,
        "tests/test_ssh_key",
        "root@localhost",
        &["docker", "ps"],
    )
    .output()
    .await
    .expect("Failed to execute SSH docker ps");
    let ps_stdout = String::from_utf8_lossy(&ps_output.stdout);
    assert!(
        ps_stdout.contains("nginx"),
        "Nginx container not found in docker ps output: {}",
        ps_stdout
    );

    // --- Teardown deployment via 'destroy' ---
    let destroy_output = Command::new("cargo")
        .args([
            "run",
            "-q",
            "--",
            "-f",
            compose_path.to_str().unwrap(),
            "-e",
            env_path.to_str().unwrap(),
            "-i",
            "tests/test_ssh_key",
            "-w",
            "/opt/test-project",
            "destroy",
            "--force",
            &target,
        ])
        .output()
        .await
        .expect("Failed to execute DCD destroy command");
    assert!(
        destroy_output.status.success(),
        "DCD destroy command failed: {}",
        String::from_utf8_lossy(&destroy_output.stderr)
    );
    let destroy_stdout = String::from_utf8_lossy(&destroy_output.stdout);
    let destroy_stderr = String::from_utf8_lossy(&destroy_output.stderr);
    assert!(
        destroy_stdout.contains("Deployment destroyed successfully")
            || destroy_stderr.contains("Deployment destroyed successfully"),
        "Unexpected destroy output:\n--- STDOUT ---\n{}\n--- STDERR ---\n{}",
        destroy_stdout,
        destroy_stderr
    );

    // Verify nginx container is no longer running
    let ps_after = ssh_cmd(
        ssh_port,
        "tests/test_ssh_key",
        "root@localhost",
        &["docker", "ps"],
    )
    .output()
    .await
    .expect("Failed to execute SSH docker ps after destroy");
    let ps_after_stdout = String::from_utf8_lossy(&ps_after.stdout);
    assert!(
        !ps_after_stdout.contains("nginx"),
        "Nginx container still running after destroy: {}",
        ps_after_stdout
    );

    // Stop helper container
    container.stop().await.unwrap();
}

