use std::borrow::Cow;
use std::path::PathBuf;
use tempfile::TempDir;
use testcontainers::core::{Image, WaitFor};
use testcontainers::{
    core::{IntoContainerPort, Mount},
    runners::AsyncRunner,
    ContainerAsync, ImageExt,
};
use tokio::fs;

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

/// Build the dcd binary and return its path
pub fn build_dcd_binary() -> PathBuf {
    let build_status = std::process::Command::new("cargo")
        .arg("build")
        .status()
        .expect("Failed to execute cargo build");
    assert!(build_status.success(), "cargo build failed");
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    PathBuf::from(manifest_dir)
        .join("target")
        .join("debug")
        .join("dcd")
}

/// A test project context with prepared files
pub struct TestProject {
    pub project_dir: PathBuf,
    pub compose_path: PathBuf,
    pub env_path: PathBuf,
    pub dcd_bin_path: PathBuf,
}

impl TestProject {
    /// Create a new test project with given compose and env file contents
    pub async fn new(compose_content: &str, env_content: &str) -> Self {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let dcd_source = PathBuf::from(&manifest_dir)
            .join("target")
            .join("debug")
            .join("dcd");
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

        let ssh_key_source = PathBuf::from(&manifest_dir).join("tests/test_ssh_key");
        let ssh_pub_source = PathBuf::from(&manifest_dir).join("tests/test_ssh_key.pub");
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
            project_dir,
            compose_path,
            env_path,
            dcd_bin_path: dcd_dest,
        }
    }
}
