use lazy_static::lazy_static;
use std::borrow::Cow;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use testcontainers::core::{Image, WaitFor};
use testcontainers::{
    core::{IntoContainerPort, Mount},
    runners::AsyncRunner,
    ContainerAsync, ImageExt,
};
use tokio::fs;
use tokio::process::Command;
use tokio::time::sleep;

/// Public key for SSH key-based authentication, read from test_ssh_key.pub.
pub const AUTHORIZED_KEY: &str = include_str!("../test_ssh_key.pub");

/// A helper SSH server container based on Debian Slim.
#[derive(Debug, Clone)]
pub struct SshServer;

impl Default for SshServer {
    fn default() -> Self {
        SshServer
    }
}

impl Image for SshServer {
    fn name(&self) -> &str {
        "debian"
    }

    fn tag(&self) -> &str {
        "12-slim"
    }

    fn ready_conditions(&self) -> Vec<WaitFor> {
        vec![WaitFor::message_on_stderr("Server listening on")]
    }

    fn env_vars(
        &self,
    ) -> impl IntoIterator<Item = (impl Into<Cow<'_, str>>, impl Into<Cow<'_, str>>)> {
        vec![(
            Cow::Borrowed("DEBIAN_FRONTEND"),
            Cow::Borrowed("noninteractive"),
        )]
    }

    fn copy_to_sources(&self) -> impl IntoIterator<Item = &testcontainers::CopyToContainer> {
        std::iter::empty()
    }

    fn cmd(&self) -> impl IntoIterator<Item = impl Into<Cow<'_, str>>> {
        let pubkey = AUTHORIZED_KEY.trim_end();
        let script = format!(
            "apt-get update && apt-get install -y openssh-server sudo curl wget gnupg2 lsb-release ca-certificates procps rsyslog apt-transport-https && \
apt-get clean && rm -rf /var/lib/apt/lists/* && \
mkdir -p /var/run/sshd && \
echo 'root:password' | chpasswd && \
sed -i 's/#PermitRootLogin prohibit-password/PermitRootLogin yes/' /etc/ssh/sshd_config && \
sed -i 's/#PasswordAuthentication yes/PasswordAuthentication yes/' /etc/ssh/sshd_config && \
mkdir -p /root/.ssh && chmod 700 /root/.ssh && \
printf '%s\\n' '{pubkey}' > /root/.ssh/authorized_keys && chmod 600 /root/.ssh/authorized_keys && \
mkdir -p /opt/test-project && /usr/sbin/sshd -D -e",
            pubkey = pubkey
        );
        vec![Cow::Borrowed("sh"), Cow::Borrowed("-c"), Cow::Owned(script)]
    }
}

/// Build an SSH command to the test SSH server.
/// Usage: common::ssh_cmd(port, key_path, target, &["docker", "ps"])
pub fn ssh_cmd(
    port: u16,
    key_path: &str,
    target: &str,
    remote_cmd: &[&str],
) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new("ssh");
    cmd.arg("-o")
        .arg("StrictHostKeyChecking=no")
        .arg("-o")
        .arg("UserKnownHostsFile=/dev/null")
        .arg("-i")
        .arg(key_path)
        .arg("-p")
        .arg(port.to_string())
        .arg(target);
    for &arg in remote_cmd {
        cmd.arg(arg);
    }
    cmd
}
/// Start SSH server container and return container and mapped host port
pub async fn start_ssh_server() -> (ContainerAsync<SshServer>, u16) {
    let container = SshServer
        .with_mapped_port(0, 22.tcp())
        .with_mount(Mount::bind_mount(
            "/var/run/docker.sock",
            "/var/run/docker.sock",
        ))
        .start()
        .await
        .expect("failed to start SSH container");
    let port = container
        .get_host_port_ipv4(22)
        .await
        .expect("SSH port not mapped");
    (container, port)
}

/// Build the dcd binary and return its path (cached; built only once)
pub fn build_dcd_binary() -> PathBuf {
    lazy_static! {
        static ref DCD_PATH: PathBuf = {
            let build_status = std::process::Command::new("cargo")
                .arg("build")
                .status()
                .expect("Failed to execute cargo build");
            assert!(build_status.success(), "cargo build failed");
            let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            manifest_dir.join("target").join("debug").join("dcd")
        };
    }
    DCD_PATH.clone()
}
/// Waits for a container to appear via SSH, polling every 0.5s until timeout.
pub async fn wait_for_container(
    ssh_port: u16,
    key_path: &str,
    target: &str,
    container_name: &str,
    timeout: Duration,
) {
    let start = Instant::now();
    loop {
        let mut cmd = ssh_cmd(
            ssh_port,
            key_path,
            target,
            &["docker", "ps", "--format", "{{.Names}}"],
        );
        let output = cmd.output().await.expect("Failed to execute SSH docker ps");
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.lines().any(|name| name.trim().contains(container_name)) {
            break;
        }
        if start.elapsed() >= timeout {
            panic!(
                "Timed out waiting for container '{}' to appear. Last output: {}",
                container_name, stdout
            );
        }
        sleep(Duration::from_millis(500)).await;
    }
}
/// Waits for a container to be removed via SSH, polling every 0.5s until timeout.
pub async fn wait_for_container_absent(
    ssh_port: u16,
    key_path: &str,
    target: &str,
    container_substring: &str,
    timeout: Duration,
) {
    let start = Instant::now();
    loop {
        let mut cmd = ssh_cmd(
            ssh_port,
            key_path,
            target,
            &["docker", "ps", "--format", "{{.Names}}", "-a"],
        );
        let output = cmd.output().await.expect("Failed to execute SSH docker ps -a");
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.contains(container_substring) {
            break;
        }
        if start.elapsed() >= timeout {
            panic!(
                "Timed out waiting for container '{}' to disappear. Last output: {}",
                container_substring, stdout
            );
        }
        sleep(Duration::from_millis(500)).await;
    }
}

/// A test project context with prepared files
pub struct TestProject {
    // Keep TempDir alive to prevent removing the directory prematurely
    _temp_dir: TempDir,
    pub project_dir: PathBuf,
    pub compose_path: PathBuf,
    pub env_path: PathBuf,
    pub dcd_bin_path: PathBuf,
    pub remote_workdir: String,
}

impl TestProject {
    /// Create a new test project with given compose and env file contents
    pub async fn new(compose_content: &str, env_content: &str, remote_workdir: &str) -> Self {
        // Use compile-time manifest directory to locate sources
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // Path to the built dcd binary
        let dcd_source = manifest_dir.join("target").join("debug").join("dcd");
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let project_dir = temp_dir.path().to_path_buf();

        let compose_path = project_dir.join("docker-compose.yml");
        fs::write(&compose_path, compose_content)
            .await
            .expect("Failed to write docker-compose.yml");

        let env_path = project_dir.join(".env");
        fs::write(&env_path, env_content)
            .await
            .expect("Failed to write .env file");

        // Paths to SSH keys for tests
        let ssh_key_source = manifest_dir.join("tests/test_ssh_key");
        let ssh_pub_source = manifest_dir.join("tests/test_ssh_key.pub");
        let ssh_key_dest = project_dir.join("test_ssh_key");
        let ssh_pub_dest = project_dir.join("test_ssh_key.pub");

        fs::copy(&ssh_key_source, &ssh_key_dest)
            .await
            .expect("Failed to copy SSH private key");
        fs::copy(&ssh_pub_source, &ssh_pub_dest)
            .await
            .expect("Failed to copy SSH public key");

        let dcd_dest = project_dir.join("dcd");
        fs::copy(&dcd_source, &dcd_dest)
            .await
            .expect("Failed to copy dcd binary");

        TestProject {
            _temp_dir: temp_dir,
            project_dir,
            compose_path,
            env_path,
            dcd_bin_path: dcd_dest,
            remote_workdir: remote_workdir.to_string(),
        }
    }

    /// Destroys the deployment and verifies that specified container names (substrings) are gone.
    pub async fn destroy(
        &self,
        target_param: &str,
        ssh_port: u16,
        expected_absent_container_substrings: &[&str],
    ) {
        let mut cmd_destroy = Command::new(&self.dcd_bin_path);
        cmd_destroy.current_dir(&self.project_dir).args([
            "-f",
            self.compose_path.to_str().unwrap(),
            "-e",
            self.env_path.to_str().unwrap(),
            "-i",
            "test_ssh_key", // Relative to project_dir
            "-w",
            &self.remote_workdir,
            "destroy",
            "--force",
            target_param,
        ]);

        let output_destroy = cmd_destroy
            .output()
            .await
            .expect("Failed to execute DCD destroy command from TestProject");

        assert!(
            output_destroy.status.success(),
            "DCD destroy command failed (from TestProject): stderr: {}, stdout: {}",
            String::from_utf8_lossy(&output_destroy.stderr),
            String::from_utf8_lossy(&output_destroy.stdout)
        );

        let stdout_destroy = String::from_utf8_lossy(&output_destroy.stdout);
        let stderr_destroy = String::from_utf8_lossy(&output_destroy.stderr);
        assert!(
            stdout_destroy.contains("Deployment destroyed successfully")
                || stderr_destroy.contains("Deployment destroyed successfully"),
            "Unexpected destroy output (from TestProject):\n--- STDOUT ---\n{}\n--- STDERR ---\n{}",
            stdout_destroy,
            stderr_destroy
        );

        // Verify all specified containers are gone, waiting dynamically
        let ssh_key_path = self.project_dir.join("test_ssh_key");
        let key_path_str = ssh_key_path.to_str().unwrap();
        for name_substring in expected_absent_container_substrings {
            wait_for_container_absent(
                ssh_port,
                key_path_str,
                "root@localhost",
                name_substring,
                Duration::from_secs(10),
            )
            .await;
        }
    }
}
