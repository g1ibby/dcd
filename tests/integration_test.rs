use std::time::Duration;
use tokio::fs;
use tokio::process::Command;
use tokio::time::sleep;

mod common;
use common::{build_dcd_binary, ssh_cmd, start_ssh_server, wait_for_container, TestProject};

#[cfg(feature = "integration-tests")]
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

#[cfg(feature = "integration-tests")]
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
    let remote_workdir = "/opt/test_dcd_up";
    let project = TestProject::new(&compose_content, "", remote_workdir).await;

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
            remote_workdir,
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

    // Wait for nginx container to start
    wait_for_container(
        ssh_port,
        "tests/test_ssh_key",
        "root@localhost",
        "nginx",
        Duration::from_secs(10),
    )
    .await;

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

    // Teardown deployment and verify
    project.destroy(&target, ssh_port, &["nginx"]).await;

    // Stop helper container
    container.stop().await.unwrap();
}

// Test deploying a container with environment variables from .env file, system env, and defaults
#[cfg(feature = "integration-tests")]
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
    let remote_workdir = "/opt/test_dcd_up_with_env_and_defaults";
    let project = TestProject::new(&compose_content, env_content, remote_workdir).await;

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
            remote_workdir,
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

    // Wait for test_env container to start
    wait_for_container(
        ssh_port,
        "tests/test_ssh_key",
        "root@localhost",
        "test_env",
        Duration::from_secs(10),
    )
    .await;

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

    // Teardown deployment and verify
    project.destroy(&target, ssh_port, &["test_env"]).await;

    container.stop().await.unwrap();
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_dcd_redeploy_with_changes() {
    let (container, ssh_port) = start_ssh_server().await;
    let _dcd_path = build_dcd_binary(); // Ensure binary is built

    // Initial project setup
    let initial_compose_content = [
        "version: '3'",
        "services:",
        "  service1:",
        "    image: busybox:latest",
        "    container_name: service1_redeploy",
        "    command: [\"sh\", \"-c\", \"sleep 3600\"]",
        "    environment:",
        "      - MY_VAR=${MY_VAR}",
    ]
    .join("\n");
    let initial_env_content = "MY_VAR=initial_value\n";
    let remote_workdir = "/opt/test_dcd_redeploy";
    let project = TestProject::new(
        &initial_compose_content,
        initial_env_content,
        remote_workdir,
    )
    .await;
    let target = format!("root@localhost:{}", ssh_port);

    // --- First Deployment ---
    let mut cmd_up1 = Command::new(&project.dcd_bin_path);
    cmd_up1.current_dir(&project.project_dir).args([
        "-f",
        project.compose_path.to_str().unwrap(),
        "-e",
        project.env_path.to_str().unwrap(),
        "-i",
        "test_ssh_key", // Relative to project_dir
        "-w",
        remote_workdir,
        "up",
        "--no-health-check",
        &target,
    ]);

    let output_up1 = cmd_up1
        .output()
        .await
        .expect("Failed to execute DCD up command (1st deploy)");
    assert!(
        output_up1.status.success(),
        "DCD up command failed (1st deploy): stderr: {}, stdout: {}",
        String::from_utf8_lossy(&output_up1.stderr),
        String::from_utf8_lossy(&output_up1.stdout)
    );
    let stdout_up1 = String::from_utf8_lossy(&output_up1.stdout);
    let stderr_up1 = String::from_utf8_lossy(&output_up1.stderr);
    assert!(
        stdout_up1.contains("Deployment successful")
            || stderr_up1.contains("Deployment successful"),
        "Unexpected output (1st deploy):\n--- STDOUT ---\n{}\n--- STDERR ---\n{}",
        stdout_up1,
        stderr_up1
    );

    // Wait for service1_redeploy container to start after first deploy
    wait_for_container(
        ssh_port,
        "tests/test_ssh_key",
        "root@localhost",
        "service1_redeploy",
        Duration::from_secs(10),
    )
    .await;

    // Verify service1 after 1st deploy
    let ps_output1 = ssh_cmd(
        ssh_port,
        "tests/test_ssh_key",
        "root@localhost",
        &["docker", "ps", "--format", "{{.Names}}"],
    )
    .output()
    .await
    .expect("SSH docker ps failed (after 1st deploy)");
    assert!(
        ps_output1.status.success(),
        "SSH docker ps command failed (after 1st deploy)"
    );
    let ps_stdout1 = String::from_utf8_lossy(&ps_output1.stdout);
    assert!(
        ps_stdout1
            .lines()
            .any(|name| name.trim() == "service1_redeploy"),
        "service1_redeploy not found after 1st deploy: {}",
        ps_stdout1
    );

    let env_output1 = ssh_cmd(
        ssh_port,
        "tests/test_ssh_key",
        "root@localhost",
        &["docker", "exec", "service1_redeploy", "env"],
    )
    .output()
    .await
    .expect("SSH docker exec env failed (service1, 1st deploy)");
    assert!(
        env_output1.status.success(),
        "SSH docker exec env command failed (service1, 1st deploy)"
    );
    let env_stdout1 = String::from_utf8_lossy(&env_output1.stdout);
    assert!(
        env_stdout1.contains("MY_VAR=initial_value"),
        "MY_VAR not 'initial_value' in service1 after 1st deploy: {}",
        env_stdout1
    );

    // --- Prepare for Redeployment ---
    // Modify docker-compose.yml: add service2, keep service1
    let updated_compose_content = [
        "version: '3'",
        "services:",
        "  service1:",
        "    image: busybox:latest", // Definition can remain the same or change
        "    container_name: service1_redeploy",
        "    command: [\"sh\", \"-c\", \"sleep 3600\"]",
        "    environment:",
        "      - MY_VAR=${MY_VAR}", // This will pick up the new .env value
        "  service2:",              // Add service2
        "    image: busybox:latest",
        "    container_name: service2_redeploy",
        "    command: [\"sh\", \"-c\", \"sleep 3600\"]",
    ]
    .join("\n");
    fs::write(&project.compose_path, updated_compose_content)
        .await
        .expect("Failed to write updated docker-compose.yml");

    // Modify .env: change MY_VAR
    let updated_env_content = "MY_VAR=updated_value\n";
    fs::write(&project.env_path, updated_env_content)
        .await
        .expect("Failed to write updated .env file");

    // --- Second Deployment (Redeploy) ---
    let mut cmd_up2 = Command::new(&project.dcd_bin_path);
    cmd_up2.current_dir(&project.project_dir).args([
        "-f",
        project.compose_path.to_str().unwrap(), // Use updated compose
        "-e",
        project.env_path.to_str().unwrap(), // Use updated env
        "-i",
        "test_ssh_key",
        "-w",
        remote_workdir,
        "up",
        "--no-health-check",
        &target,
    ]);

    let output_up2 = cmd_up2
        .output()
        .await
        .expect("Failed to execute DCD up command (redeploy)");
    assert!(
        output_up2.status.success(),
        "DCD up command failed (redeploy): stderr: {}, stdout: {}",
        String::from_utf8_lossy(&output_up2.stderr),
        String::from_utf8_lossy(&output_up2.stdout)
    );
    let stdout_up2 = String::from_utf8_lossy(&output_up2.stdout);
    let stderr_up2 = String::from_utf8_lossy(&output_up2.stderr);
    assert!(
        stdout_up2.contains("Deployment successful")
            || stderr_up2.contains("Deployment successful"),
        "Unexpected output (redeploy):\n--- STDOUT ---\n{}\n--- STDERR ---\n{}",
        stdout_up2,
        stderr_up2
    );

    // Wait for service1_redeploy and service2_redeploy containers to start after redeploy
    wait_for_container(
        ssh_port,
        "tests/test_ssh_key",
        "root@localhost",
        "service1_redeploy",
        Duration::from_secs(15),
    )
    .await;
    wait_for_container(
        ssh_port,
        "tests/test_ssh_key",
        "root@localhost",
        "service2_redeploy",
        Duration::from_secs(15),
    )
    .await;

    // Verify after redeploy
    let ps_output2 = ssh_cmd(
        ssh_port,
        "tests/test_ssh_key",
        "root@localhost",
        &["docker", "ps", "--format", "{{.Names}}"],
    )
    .output()
    .await
    .expect("SSH docker ps failed (after redeploy)");
    assert!(
        ps_output2.status.success(),
        "SSH docker ps command failed (after redeploy)"
    );
    let ps_stdout2 = String::from_utf8_lossy(&ps_output2.stdout);

    // Check service1 (still running, env updated)
    assert!(
        ps_stdout2
            .lines()
            .any(|name| name.trim() == "service1_redeploy"),
        "service1_redeploy not found after redeploy: {}",
        ps_stdout2
    );
    let env_output2_service1 = ssh_cmd(
        ssh_port,
        "tests/test_ssh_key",
        "root@localhost",
        &["docker", "exec", "service1_redeploy", "env"],
    )
    .output()
    .await
    .expect("SSH docker exec env failed (service1, redeploy)");
    assert!(
        env_output2_service1.status.success(),
        "SSH docker exec env command failed (service1, redeploy)"
    );
    let env_stdout2_service1 = String::from_utf8_lossy(&env_output2_service1.stdout);
    assert!(
        env_stdout2_service1.contains("MY_VAR=updated_value"),
        "MY_VAR not 'updated_value' in service1 after redeploy: {}",
        env_stdout2_service1
    );

    // Check service2 (newly added)
    assert!(
        ps_stdout2
            .lines()
            .any(|name| name.trim() == "service2_redeploy"),
        "service2_redeploy not found after redeploy: {}",
        ps_stdout2
    );

    // --- Teardown ---
    project
        .destroy(
            &target,
            ssh_port,
            &["service1_redeploy", "service2_redeploy"],
        )
        .await;

    container.stop().await.unwrap();
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_dcd_compose_profiles() {
    let (container, ssh_port) = start_ssh_server().await;
    let _dcd_path = build_dcd_binary();

    // Create docker-compose.yml with two services:
    // - web_service: no profile (always runs)
    // - dev_service: has "development" profile (only runs when profile is active)
    let compose_content = [
        "version: '3'",
        "services:",
        "  web_service:",
        "    image: busybox:latest",
        "    container_name: web_profile_test",
        "    command: [\"sh\", \"-c\", \"sleep 3600\"]",
        "    environment:",
        "      - SERVICE_NAME=web",
        "  dev_service:",
        "    image: busybox:latest",
        "    container_name: dev_profile_test",
        "    command: [\"sh\", \"-c\", \"sleep 3600\"]",
        "    environment:",
        "      - SERVICE_NAME=dev",
        "    profiles:",
        "      - development",
    ]
    .join("\n");

    let env_content = ""; // No env vars needed for this test
    let remote_workdir = "/opt/test_dcd_profiles";
    let project = TestProject::new(&compose_content, env_content, remote_workdir).await;
    let target = format!("root@localhost:{}", ssh_port);

    // --- Phase 1: Deploy WITHOUT COMPOSE_PROFILES ---
    // Only web_service should start (no profile required)
    let mut cmd_up1 = Command::new(&project.dcd_bin_path);
    cmd_up1.current_dir(&project.project_dir).args([
        "-f",
        project.compose_path.to_str().unwrap(),
        "-e",
        project.env_path.to_str().unwrap(),
        "-i",
        "test_ssh_key",
        "-w",
        remote_workdir,
        "up",
        "--no-health-check",
        &target,
    ]);

    let output_up1 = cmd_up1
        .output()
        .await
        .expect("Failed to execute DCD up command (without profiles)");
    assert!(
        output_up1.status.success(),
        "DCD up command failed (without profiles): stderr: {}, stdout: {}",
        String::from_utf8_lossy(&output_up1.stderr),
        String::from_utf8_lossy(&output_up1.stdout)
    );

    let stdout_up1 = String::from_utf8_lossy(&output_up1.stdout);
    let stderr_up1 = String::from_utf8_lossy(&output_up1.stderr);
    assert!(
        stdout_up1.contains("Deployment successful")
            || stderr_up1.contains("Deployment successful"),
        "Unexpected output (without profiles):\n--- STDOUT ---\n{}\n--- STDERR ---\n{}",
        stdout_up1,
        stderr_up1
    );

    // Wait for web_profile_test container to start
    wait_for_container(
        ssh_port,
        "tests/test_ssh_key",
        "root@localhost",
        "web_profile_test",
        Duration::from_secs(10),
    )
    .await;

    // Verify only web_service is running (dev_service should not be running)
    let ps_output1 = ssh_cmd(
        ssh_port,
        "tests/test_ssh_key",
        "root@localhost",
        &["docker", "ps", "--format", "{{.Names}}"],
    )
    .output()
    .await
    .expect("SSH docker ps failed (after deploy without profiles)");

    let ps_stdout1 = String::from_utf8_lossy(&ps_output1.stdout);
    assert!(
        ps_stdout1
            .lines()
            .any(|name| name.trim() == "web_profile_test"),
        "web_profile_test container not found (without profiles): {}",
        ps_stdout1
    );
    assert!(
        !ps_stdout1
            .lines()
            .any(|name| name.trim() == "dev_profile_test"),
        "dev_profile_test container should not be running (without profiles): {}",
        ps_stdout1
    );

    // Verify web_service environment
    let env_output1 = ssh_cmd(
        ssh_port,
        "tests/test_ssh_key",
        "root@localhost",
        &["docker", "exec", "web_profile_test", "env"],
    )
    .output()
    .await
    .expect("SSH docker exec env failed (web_service, without profiles)");

    let env_stdout1 = String::from_utf8_lossy(&env_output1.stdout);
    assert!(
        env_stdout1.contains("SERVICE_NAME=web"),
        "SERVICE_NAME not 'web' in web_service (without profiles): {}",
        env_stdout1
    );

    // --- Phase 2: Redeploy WITH COMPOSE_PROFILES=development ---
    // Both web_service and dev_service should start
    let mut cmd_up2 = Command::new(&project.dcd_bin_path);
    cmd_up2
        .current_dir(&project.project_dir)
        .env("COMPOSE_PROFILES", "development") // Set the profiles environment variable
        .args([
            "-f",
            project.compose_path.to_str().unwrap(),
            "-e",
            project.env_path.to_str().unwrap(),
            "-i",
            "test_ssh_key",
            "-w",
            remote_workdir,
            "up",
            "--no-health-check",
            &target,
        ]);

    let output_up2 = cmd_up2
        .output()
        .await
        .expect("Failed to execute DCD up command (with profiles)");
    assert!(
        output_up2.status.success(),
        "DCD up command failed (with profiles): stderr: {}, stdout: {}",
        String::from_utf8_lossy(&output_up2.stderr),
        String::from_utf8_lossy(&output_up2.stdout)
    );

    let stdout_up2 = String::from_utf8_lossy(&output_up2.stdout);
    let stderr_up2 = String::from_utf8_lossy(&output_up2.stderr);
    assert!(
        stdout_up2.contains("Deployment successful")
            || stderr_up2.contains("Deployment successful"),
        "Unexpected output (with profiles):\n--- STDOUT ---\n{}\n--- STDERR ---\n{}",
        stdout_up2,
        stderr_up2
    );

    // Wait for web_profile_test and dev_profile_test containers to start
    wait_for_container(
        ssh_port,
        "tests/test_ssh_key",
        "root@localhost",
        "web_profile_test",
        Duration::from_secs(10),
    )
    .await;
    wait_for_container(
        ssh_port,
        "tests/test_ssh_key",
        "root@localhost",
        "dev_profile_test",
        Duration::from_secs(10),
    )
    .await;

    // Verify both web_service and dev_service are running
    let ps_output2 = ssh_cmd(
        ssh_port,
        "tests/test_ssh_key",
        "root@localhost",
        &["docker", "ps", "--format", "{{.Names}}"],
    )
    .output()
    .await
    .expect("SSH docker ps failed (after deploy with profiles)");

    let ps_stdout2 = String::from_utf8_lossy(&ps_output2.stdout);
    assert!(
        ps_stdout2
            .lines()
            .any(|name| name.trim() == "web_profile_test"),
        "web_profile_test container not found (with profiles): {}",
        ps_stdout2
    );
    assert!(
        ps_stdout2
            .lines()
            .any(|name| name.trim() == "dev_profile_test"),
        "dev_profile_test container not found (with profiles): {}",
        ps_stdout2
    );

    // Verify dev_service environment
    let env_output2 = ssh_cmd(
        ssh_port,
        "tests/test_ssh_key",
        "root@localhost",
        &["docker", "exec", "dev_profile_test", "env"],
    )
    .output()
    .await
    .expect("SSH docker exec env failed (dev_service, with profiles)");

    let env_stdout2 = String::from_utf8_lossy(&env_output2.stdout);
    assert!(
        env_stdout2.contains("SERVICE_NAME=dev"),
        "SERVICE_NAME not 'dev' in dev_service (with profiles): {}",
        env_stdout2
    );

    // --- Teardown ---
    project
        .destroy(&target, ssh_port, &["web_profile_test", "dev_profile_test"])
        .await;

    container.stop().await.unwrap();
}
