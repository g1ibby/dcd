use std::time::Duration;
use tokio::process::Command;
use tokio::time::sleep;

mod common;
use common::{build_dcd_binary, ssh_cmd, start_ssh_server, TestProject};

#[tokio::test]
async fn test_ssh_server() {
    let (container, host_port) = start_ssh_server().await;

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

    container.stop().await.unwrap();
}

#[tokio::test]
async fn test_dcd_up() {
    let (container, ssh_port) = start_ssh_server().await;

    // Build the project's binary
    let _dcd_path = build_dcd_binary();

    // Prepare temporary project directory with a simple nginx compose file
    let compose_content = [
        "version: '3'",
        "services:",
        "  nginx:",
        "    image: nginx:alpine",
        "    ports:",
        "      - \"8080:80\"",
    ]
    .join("\n");
    let project = TestProject::new(&compose_content, "").await;

    // Run the DCD up command to deploy nginx
    let target = format!("root@localhost:{}", ssh_port);
    let mut cmd = Command::new(&project.dcd_bin_path);
    cmd.current_dir(&project.project_dir)
        .env("SYSTEM_VAR", "sys_val")
        .args([
            "-f",
            project.compose_path.to_str().unwrap(),
            "-e",
            project.env_path.to_str().unwrap(),
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

    // Teardown deployment via 'destroy'
    let destroy_output = Command::new("cargo")
        .args([
            "run",
            "-q",
            "--",
            "-f",
            project.compose_path.to_str().unwrap(),
            "-e",
            project.env_path.to_str().unwrap(),
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
    let (container, ssh_port) = start_ssh_server().await;

    // Build the project's binary
    let _dcd_path = build_dcd_binary();

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
    let env_content = "FILE_VAR=file_val\n";
    let project = TestProject::new(&compose_content, env_content).await;

    // Run the DCD up command with --no-health-check, setting SYSTEM_VAR in environment
    let target = format!("root@localhost:{}", ssh_port);
    let mut cmd = Command::new(&project.dcd_bin_path);
    cmd.current_dir(&project.project_dir)
        .env("SYSTEM_VAR", "sys_val")
        .args([
            "-f",
            project.compose_path.to_str().unwrap(),
            "-e",
            project.env_path.to_str().unwrap(),
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
    let destroy_output = Command::new(&project.dcd_bin_path)
        .current_dir(&project.project_dir)
        .args([
            "-f",
            project.compose_path.to_str().unwrap(),
            "-e",
            project.env_path.to_str().unwrap(),
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

    container.stop().await.unwrap();
}
