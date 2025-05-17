use std::borrow::Cow;
use testcontainers::core::{Image, WaitFor};

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

