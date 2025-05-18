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
async fn test_dcd_up() {
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

    // Build the project's binary
    let build_status = std::process::Command::new("cargo")
        .arg("build")
        .status()
        .expect("Failed to execute cargo build");
    assert!(build_status.success(), "cargo build failed");

    // Determine the path to the built binary
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let dcd_binary_source_path = std::path::PathBuf::from(manifest_dir.clone())
        .join("target")
        .join("debug")
        .join("dcd");

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

    // Copy SSH keys to the project directory
    let ssh_key_source_path = std::path::PathBuf::from(&manifest_dir).join("tests/test_ssh_key");
    let ssh_key_pub_source_path =
        std::path::PathBuf::from(&manifest_dir).join("tests/test_ssh_key.pub");

    let ssh_key_dest_path = project_dir.join("test_ssh_key");
    let ssh_key_pub_dest_path = project_dir.join("test_ssh_key.pub");

    fs::copy(&ssh_key_source_path, &ssh_key_dest_path)
        .await
        .expect("Failed to copy SSH private key to project_dir");
    fs::copy(&ssh_key_pub_source_path, &ssh_key_pub_dest_path)
        .await
        .expect("Failed to copy SSH public key to project_dir");

    // Copy the dcd binary to the project directory
    let dcd_binary_dest_path = project_dir.join("dcd");
    fs::copy(&dcd_binary_source_path, &dcd_binary_dest_path)
        .await
        .expect("Failed to copy dcd binary to project_dir");

    // Run the DCD up command to deploy nginx
    let target = format!("root@localhost:{}", ssh_port);
    let mut cmd = Command::new(&dcd_binary_dest_path);
    cmd.current_dir(project_dir); // Run from within project_dir
    cmd.env("SYSTEM_VAR", "sys_val").args([
        "-f",
        compose_path.to_str().unwrap(),
        "-e",
        env_path.to_str().unwrap(),
        "-i",
        "test_ssh_key",
        "-w",
        "/opt/test_dcd_up",
        "up",
        "--no-health-check",
        &target,
    ]);
    let output = cmd
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
    // The success message may be logged to stderr or stdout
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
            "/opt/test_dcd_up",
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

// Test deploying a container with environment variables from .env file, system env, and defaults
#[tokio::test]
async fn test_dcd_up_with_env_and_defaults() {
    // --- Setup: Build the dcd binary ---
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

    // Build the project's binary
    let build_status = std::process::Command::new("cargo")
        .arg("build")
        .status()
        .expect("Failed to execute cargo build");
    assert!(build_status.success(), "cargo build failed");

    // Determine the path to the built binary
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let dcd_binary_source_path = std::path::PathBuf::from(manifest_dir.clone())
        .join("target")
        .join("debug")
        .join("dcd");

    // Prepare temporary project directory
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_dir = temp_dir.path();
    // Write docker-compose.yml with env vars and default
    let compose_path = project_dir.join("docker-compose.yml");
    let compose_content = [
        "version: '3'",
        "services:",
        "  test:",
        "    image: busybox:latest",
        "    container_name: test_env",
        "    command: [\"sh\", \"-c\", \"sleep 3600\"]",
        "    environment:",
        "      - FILE_VAR=${FILE_VAR}",
        "      - SYSTEM_VAR=${SYSTEM_VAR}",
        "      - DEFAULT_VAR=${DEFAULT_VAR:-def123}",
    ]
    .join("\n");
    fs::write(&compose_path, compose_content)
        .await
        .expect("Failed to write docker-compose.yml");

    // Write .env file with FILE_VAR
    let env_path = project_dir.join(".env");
    fs::write(&env_path, "FILE_VAR=file_val\n")
        .await
        .expect("Failed to write .env file");

    // Copy SSH keys to the project directory
    let ssh_key_source_path = std::path::PathBuf::from(&manifest_dir).join("tests/test_ssh_key");
    let ssh_key_pub_source_path =
        std::path::PathBuf::from(&manifest_dir).join("tests/test_ssh_key.pub");

    let ssh_key_dest_path = project_dir.join("test_ssh_key");
    let ssh_key_pub_dest_path = project_dir.join("test_ssh_key.pub");

    fs::copy(&ssh_key_source_path, &ssh_key_dest_path)
        .await
        .expect("Failed to copy SSH private key to project_dir");
    fs::copy(&ssh_key_pub_source_path, &ssh_key_pub_dest_path)
        .await
        .expect("Failed to copy SSH public key to project_dir");

    // Copy the dcd binary to the project directory
    let dcd_binary_dest_path = project_dir.join("dcd");
    fs::copy(&dcd_binary_source_path, &dcd_binary_dest_path)
        .await
        .expect("Failed to copy dcd binary to project_dir");

    // Run the DCD up command with --no-health-check, setting SYSTEM_VAR in environment
    let target = format!("root@localhost:{}", ssh_port);
    let mut cmd = Command::new(&dcd_binary_dest_path);
    cmd.current_dir(project_dir); // Run from within project_dir
    cmd.env("SYSTEM_VAR", "sys_val").args([
        "-f",
        compose_path.to_str().unwrap(),
        "-e",
        env_path.to_str().unwrap(),
        "-i",
        "test_ssh_key",
        "-w",
        "/opt/test_dcd_up_with_env_and_defaults",
        "up",
        "--no-health-check",
        &target,
    ]);
    let output = cmd
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
    // The success message may be logged to stderr or stdout
    assert!(
        stdout.contains("Deployment successful") || stderr.contains("Deployment successful"),
        "Unexpected output:\n--- STDOUT ---\n{}\n--- STDERR ---\n{}",
        stdout,
        stderr
    );

    // Allow some time for Docker Compose to start containers
    sleep(Duration::from_secs(5)).await;

    // Verify test_env container is running via SSH
    let ps_output = ssh_cmd(
        ssh_port,
        "tests/test_ssh_key",
        "root@localhost",
        &["docker", "ps", "--format", "{{.Names}}"],
    )
    .output()
    .await
    .expect("Failed to execute SSH docker ps");
    let ps_stdout = String::from_utf8_lossy(&ps_output.stdout);
    assert!(
        ps_stdout.lines().any(|name| name.trim() == "test_env"),
        "test_env container not found: {}",
        ps_stdout
    );

    // Verify environment variables inside the container
    let env_output = ssh_cmd(
        ssh_port,
        "tests/test_ssh_key",
        "root@localhost",
        &["docker", "exec", "test_env", "env"],
    )
    .output()
    .await
    .expect("Failed to execute SSH docker exec env");
    let env_stdout = String::from_utf8_lossy(&env_output.stdout);
    assert!(
        env_stdout.contains("FILE_VAR=file_val"),
        "FILE_VAR not set: {}",
        env_stdout
    );
    assert!(
        env_stdout.contains("SYSTEM_VAR=sys_val"),
        "SYSTEM_VAR not set: {}",
        env_stdout
    );
    assert!(
        env_stdout.contains("DEFAULT_VAR=def123"),
        "DEFAULT_VAR default not set: {}",
        env_stdout
    );

    // Teardown deployment via 'destroy'
    let destroy_output = Command::new(&dcd_binary_dest_path)
        .current_dir(project_dir)
        .args([
            "-f",
            compose_path.to_str().unwrap(),
            "-e",
            env_path.to_str().unwrap(),
            "-i",
            "test_ssh_key",
            "-w",
            "/opt/test_dcd_up_with_env_and_defaults",
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

    // Stop helper container
    container.stop().await.unwrap();
}
