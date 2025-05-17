use std::borrow::Cow;
use std::time::Duration;
use tempfile::TempDir;
use testcontainers::{
    core::{Image, IntoContainerPort, Mount, WaitFor},
    runners::AsyncRunner,
    ImageExt,
};
use tokio::fs;
use tokio::process::Command;
use tokio::time::sleep;

/// Custom image that sets up an SSH server based on Debian Slim.
#[derive(Debug, Clone)]
struct SshServer;

const IMAGE_NAME: &str = "debian";
const IMAGE_TAG: &str = "12-slim";

// Public key for SSH key-based authentication.
const AUTHORIZED_KEY: &str = "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQDJ/9D59ydMYuLJWo4EO+g2Ii1VmUQe66oAHykKgWLjSH6SjuYxib98COftUmu3olyHZAPzmfXwGMyqbgjYL6b08ttrCWlBo4EOmxNEo16FOhRFhxbpxn7tPmpjLG95nu2Pk5W2KT7UEBKMVDmXZ/sRJSbWHXENuQL2bOeCMsazdykp/yrROu4S1xIoLpZYtmenZNv6r5fnMbW27ouZjOSzKKaCZjFv0xkH0CsNU2MZEFn1AH3p0PQxz9La5vRjWZlraY3594D1W0FEto07UliObhiU/Gxvml5KebJEdUrPPt51Rt3Qg8tIEponZsBuPMQo2cYe7/TRw7MkbxwNU7dJ user@mini";

impl Default for SshServer {
    fn default() -> Self {
        SshServer
    }
}

impl Image for SshServer {
    fn name(&self) -> &str {
        IMAGE_NAME
    }

    fn tag(&self) -> &str {
        IMAGE_TAG
    }

    fn ready_conditions(&self) -> Vec<WaitFor> {
        // Wait until SSH server is listening (logs to stderr with -e)
        vec![WaitFor::message_on_stderr("Server listening on")]
    }

    fn env_vars(
        &self,
    ) -> impl IntoIterator<Item = (impl Into<Cow<'_, str>>, impl Into<Cow<'_, str>>)> {
        // Prevent interactive prompts during apt-get
        vec![(
            Cow::Borrowed("DEBIAN_FRONTEND"),
            Cow::Borrowed("noninteractive"),
        )]
    }

    fn copy_to_sources(&self) -> impl IntoIterator<Item = &testcontainers::CopyToContainer> {
        // No files to copy via Docker API; we inject keys in startup script
        std::iter::empty()
    }

    fn cmd(&self) -> impl IntoIterator<Item = impl Into<Cow<'_, str>>> {
        // Build startup script with package install, config, and key injection.
        let pubkey = AUTHORIZED_KEY;
        let script = format!(
            "apt-get update && apt-get install -y openssh-server sudo curl wget gnupg2 lsb-release ca-certificates procps rsyslog apt-transport-https && \
apt-get clean && rm -rf /var/lib/apt/lists/* && \
mkdir -p /var/run/sshd && \
echo 'root:password' | chpasswd && \
sed -i 's/#PermitRootLogin prohibit-password/PermitRootLogin yes/' /etc/ssh/sshd_config && \
sed -i 's/#PasswordAuthentication yes/PasswordAuthentication yes/' /etc/ssh/sshd_config && \
mkdir -p /root/.ssh && chmod 700 /root/.ssh && \
printf '%s\n' '{pubkey}' > /root/.ssh/authorized_keys && chmod 600 /root/.ssh/authorized_keys && \
mkdir -p /opt/test-project && /usr/sbin/sshd -D -e",
            pubkey = pubkey
        );
        vec![Cow::Borrowed("sh"), Cow::Borrowed("-c"), Cow::Owned(script)]
    }
}

#[tokio::test]
async fn test_ssh_server() {
    // Start the SSH server container
    let container = SshServer
        .with_mapped_port(0, 22.tcp())
        // bind-mount the host Docker socket so the container can use the host daemon
        .with_mount(Mount::bind_mount(
            "/var/run/docker.sock",
            "/var/run/docker.sock",
        ))
        .start()
        .await
        .expect("failed to start SSH container");

    // Verify that the SSH port is mapped
    let host_port = container
        .get_host_port_ipv4(22)
        .await
        .expect("SSH port not mapped");
    assert!(
        host_port > 0,
        "SSH server port mapping should be a positive port"
    );

    // Try SSH connection with key-based authentication
    let status = Command::new("ssh")
        .args([
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-i",
            "tests/test_ssh_key",
            "-p",
            &host_port.to_string(),
            "root@127.0.0.1",
            "echo",
            "hello",
        ])
        .status()
        .await
        .expect("failed to execute ssh command");
    assert!(status.success(), "SSH command failed");

    // Verify output
    let output = Command::new("ssh")
        .args([
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-i",
            "tests/test_ssh_key",
            "-p",
            &host_port.to_string(),
            "root@127.0.0.1",
            "echo",
            "hello",
        ])
        .output()
        .await
        .expect("failed to execute ssh command");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "hello");

    // Stop the container
    container.stop().await.unwrap();
}

/// Test deployment of a simple nginx service via the `up` command against the SSH server
#[tokio::test]
async fn test_dcd_up_deploy_nginx() {
    // Start SSH server container
    let container = SshServer
        .with_mapped_port(0, 22.tcp())
        // bind-mount the host Docker socket so the container can use the host daemon
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
    let ps_output = Command::new("ssh")
        .args([
            "-i",
            "tests/test_ssh_key",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-p",
            &ssh_port.to_string(),
            "root@localhost",
            "docker",
            "ps",
        ])
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
    // Run the DCD destroy command to clean up
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
    // Should exit successfully
    assert!(
        destroy_output.status.success(),
        "DCD destroy command failed: {}",
        String::from_utf8_lossy(&destroy_output.stderr)
    );
    let destroy_stdout = String::from_utf8_lossy(&destroy_output.stdout);
    let destroy_stderr = String::from_utf8_lossy(&destroy_output.stderr);
    // Check for a successful teardown message
    assert!(
        destroy_stdout.contains("Deployment destroyed successfully")
            || destroy_stderr.contains("Deployment destroyed successfully"),
        "Unexpected destroy output:\n--- STDOUT ---\n{}\n--- STDERR ---\n{}",
        destroy_stdout,
        destroy_stderr
    );

    // Verify nginx container is no longer running
    let ps_after = Command::new("ssh")
        .args([
            "-i",
            "tests/test_ssh_key",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-p",
            &ssh_port.to_string(),
            "root@localhost",
            "docker",
            "ps",
        ])
        .output()
        .await
        .expect("Failed to execute SSH docker ps after destroy");
    let ps_after_stdout = String::from_utf8_lossy(&ps_after.stdout);
    assert!(
        !ps_after_stdout.contains("nginx"),
        "Nginx container still running after destroy: {}",
        ps_after_stdout
    );

    // Finally, stop the SSH helper container
    container.stop().await.unwrap();
}
