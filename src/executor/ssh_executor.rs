use super::error::ExecutorError;
use super::traits::{CommandExecutor, FileTransfer};
use super::types::{CommandOutput, CommandResult};
use anyhow::Result;
use async_trait::async_trait;
use colored::*;
use dirs;
use russh::keys::PublicKeyBase64;
use russh::{client, keys, ChannelMsg, Disconnect};
use russh_sftp::{client::SftpSession, protocol::OpenFlags};
use std::{collections::HashMap, path::Path, sync::Arc, time::Duration};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

/// Prints a formatted error message for a host key mismatch to stderr.
fn print_host_key_mismatch_error(host: &str, fingerprint: &str) {
    eprintln!(
        "{}\n{}\nHost: {}\nPresented Key Fingerprint (SHA256): {}\n{}\n{}\n{}",
        "!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!".red().bold(),
        "ERROR: HOST KEY VERIFICATION FAILED!".red().bold(),
        host.cyan(),
        fingerprint.yellow(),
        "The presented key does NOT MATCH any known key for this host.".red(),
        "This could mean an attacker is intercepting your connection!\nConnection rejected. Check your known_hosts file and the server's configuration.".red(),
        "!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!".red().bold()
    );
}

/// Prints a formatted warning message for an unknown host key to stderr.
fn print_unknown_host_key_warning(host: &str, fingerprint: &str, key_base64: &str) {
    eprintln!(
        "{}\n{}\nHost: {}\nKey Fingerprint (SHA256): {}\n{}\n{}\n{}",
        "!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!"
            .yellow()
            .bold(),
        "WARNING: UNKNOWN HOST KEY DETECTED!".yellow().bold(),
        host.cyan(),
        fingerprint.yellow(),
        "Connecting anyway, but be aware of potential Man-in-the-Middle attacks.".yellow(),
        format!(
            "Add the key to your known_hosts file ('{} {}') to trust it.",
            host.cyan(),
            key_base64.green()
        )
        .yellow(),
        "!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!"
            .yellow()
            .bold()
    );
}

/// A simple client handler that *always* accepts the server key.
/// In production, you should do proper host-key checking.
#[derive(Debug)]
struct ClientHandler {
    /// The hostname or IP address the client intended to connect to.
    target_host: String,
    /// Keys loaded from the known_hosts file.
    trusted_keys: HashMap<String, Vec<keys::PublicKey>>,
}

impl ClientHandler {
    /// Create a new client handler with the given target host and trusted keys.
    fn new(target_host: String, trusted_keys: HashMap<String, Vec<keys::PublicKey>>) -> Self {
        Self {
            target_host,
            trusted_keys,
        }
    }
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        let fingerprint = server_public_key.fingerprint(Default::default());
        let fingerprint_str = fingerprint.to_string();

        match self.trusted_keys.get(&self.target_host) {
            Some(known_keys_for_host) => {
                // Host IS in known_hosts, check if the presented key matches any known key
                if known_keys_for_host
                    .iter()
                    .any(|known_key| known_key == server_public_key)
                {
                    // Key matches a known key for this host.
                    tracing::debug!(
                        "Host key for {} verified (fingerprint: {}).",
                        self.target_host,
                        fingerprint
                    );
                    Ok(true)
                } else {
                    // Key MISMATCH! This is a potential security risk (MitM attack).
                    print_host_key_mismatch_error(&self.target_host, &fingerprint_str);
                    Ok(false) // Reject the connection due to key mismatch
                }
            }
            None => {
                print_unknown_host_key_warning(
                    &self.target_host,
                    &fingerprint_str,
                    &server_public_key.public_key_base64(),
                );
                Ok(true)
            }
        }
    }
}

/// The underlying SSH client that manages the russh connection and optional SFTP session.
pub struct SshClient {
    session: client::Handle<ClientHandler>,
    sftp: Arc<Mutex<Option<SftpSession>>>,
}

impl SshClient {
    /// Establish an SSH connection using the given private key, username, and address.
    pub async fn connect<A: tokio::net::ToSocketAddrs>(
        key_path: impl AsRef<Path>,
        username: &str,
        addr: A,
        target_host_str: String,
        known_hosts_map: HashMap<String, Vec<keys::PublicKey>>,
        timeout: Duration,
    ) -> Result<Self, ExecutorError> {
        let key_pair = keys::load_secret_key(key_path.as_ref(), None)
            .map_err(|e| ExecutorError::SshError(e.to_string()))?;

        let config = client::Config {
            inactivity_timeout: Some(timeout),
            ..Default::default()
        };
        let config = Arc::new(config);
        // Create the handler with the resolved host and loaded keys provided by caller
        let handler = ClientHandler::new(target_host_str, known_hosts_map);

        let mut session = client::connect(config, addr, handler)
            .await
            .map_err(|e| ExecutorError::SshError(e.to_string()))?;

        // Get the best supported RSA hash algorithm, falling back to SHA1 if server doesn't support negotiation
        let best_hash = session
            .best_supported_rsa_hash()
            .await
            .map_err(|e| ExecutorError::SshError(format!("Failed to get best RSA hash: {}", e)))?
            .flatten();

        tracing::debug!("Using RSA hash algorithm: {:?}", best_hash);

        // Authenticate using the private key
        let auth_result = session
            .authenticate_publickey(
                username,
                keys::key::PrivateKeyWithHashAlg::new(
                    Arc::new(key_pair),
                    best_hash, // Use the negotiated hash (or None for non-RSA or SHA1 fallback)
                ),
            )
            .await
            .map_err(|e| ExecutorError::SshError(e.to_string()))?;

        if !auth_result.success() {
            return Err(ExecutorError::SshError("Authentication failed".to_string()));
        }

        Ok(Self {
            session,
            sftp: Arc::new(Mutex::new(None)),
        })
    }

    /// If not already present, create an SFTP session and store it for reuse.
    async fn get_sftp_session(&self) -> Result<Arc<Mutex<Option<SftpSession>>>, ExecutorError> {
        {
            let sftp_guard = self.sftp.lock().await;
            if sftp_guard.is_some() {
                return Ok(self.sftp.clone());
            }
        }

        let channel = self
            .session
            .channel_open_session()
            .await
            .map_err(|e| ExecutorError::SshError(e.to_string()))?;

        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| ExecutorError::SshError(e.to_string()))?;

        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| ExecutorError::SshError(e.to_string()))?;

        let mut guard = self.sftp.lock().await;
        *guard = Some(sftp);

        Ok(self.sftp.clone())
    }

    /// Internal helper for uploading a file via SFTP.
    async fn upload_file_internal(
        &self,
        local_path: &Path,
        remote_path: &Path,
    ) -> Result<(), ExecutorError> {
        let sftp_session = self.get_sftp_session().await?;
        let mut sftp_guard = sftp_session.lock().await;
        let sftp = sftp_guard
            .as_mut()
            .ok_or_else(|| ExecutorError::SshError("SFTP session not available".to_string()))?;

        let mut local_file = tokio::fs::File::open(local_path)
            .await
            .map_err(|e| ExecutorError::SshError(e.to_string()))?;

        let remote_str = remote_path
            .to_str()
            .ok_or_else(|| ExecutorError::SshError("Invalid UTF-8 in remote path".to_string()))?;

        let mut remote_file = sftp
            .open_with_flags(
                remote_str,
                OpenFlags::CREATE | OpenFlags::WRITE | OpenFlags::TRUNCATE,
            )
            .await
            .map_err(|e| ExecutorError::SshError(e.to_string()))?;

        let mut buffer = Vec::new();
        local_file
            .read_to_end(&mut buffer)
            .await
            .map_err(|e| ExecutorError::SshError(e.to_string()))?;

        remote_file
            .write_all(&buffer)
            .await
            .map_err(|e| ExecutorError::SshError(e.to_string()))?;

        remote_file
            .flush()
            .await
            .map_err(|e| ExecutorError::SshError(e.to_string()))?;

        Ok(())
    }

    /// Internal helper for executing a command over SSH.
    async fn execute_command_internal(
        &mut self,
        command: &str,
    ) -> Result<CommandResult, ExecutorError> {
        let mut channel = self
            .session
            .channel_open_session()
            .await
            .map_err(|e| ExecutorError::SshError(e.to_string()))?;

        channel
            .exec(true, command)
            .await
            .map_err(|e| ExecutorError::SshError(e.to_string()))?;

        let mut output = CommandOutput::new();

        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } => {
                    output.stdout.extend_from_slice(&data);
                }
                ChannelMsg::ExtendedData { data, .. } => {
                    output.stderr.extend_from_slice(&data);
                }
                ChannelMsg::ExitStatus { exit_status } => {
                    output.exit_code = exit_status;
                }
                _ => {}
            }
        }
        output.stop_timing();

        tracing::debug!(
            "SSH Command '{}' completed with exit code {}",
            command,
            output.exit_code
        );

        Ok(CommandResult {
            command: command.to_string(),
            output,
        })
    }

    /// Internal helper for disconnecting cleanly from the SSH session.
    async fn close_internal(&mut self) -> Result<(), ExecutorError> {
        self.session
            .disconnect(Disconnect::ByApplication, "", "English")
            .await
            .map_err(|e| ExecutorError::SshError(e.to_string()))
    }
}

/// Parses a single line from a known_hosts file.
fn parse_known_host_line(line: &str) -> Option<(Vec<String>, keys::PublicKey)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 3 {
        return None; // Not enough parts
    }

    let hosts_part = parts[0];
    let key_data = parts[2];

    let hosts = hosts_part.split(',').map(String::from).collect();

    match keys::parse_public_key_base64(key_data) {
        Ok(key) => Some((hosts, key)),
        Err(_) => {
            tracing::warn!(
                "Failed to parse public key from known_hosts line '{}'",
                line
            );
            None
        }
    }
}

/// Loads known hosts from a specified file path.
/// Returns a map where keys are hostnames/IPs and values are lists of valid public keys.
async fn load_known_hosts(
    path: &Path,
) -> Result<HashMap<String, Vec<keys::PublicKey>>, ExecutorError> {
    let mut trusted_keys: HashMap<String, Vec<keys::PublicKey>> = HashMap::new();

    if !path.exists() {
        tracing::warn!(
            "Known hosts file not found at '{}'. No host keys will be pre-trusted.",
            path.display()
        );
        return Ok(trusted_keys); // Return empty map if file doesn't exist
    }

    let content = fs::read_to_string(path).await.map_err(|e| {
        ExecutorError::SshError(format!(
            "Failed to read known_hosts file '{}': {}",
            path.display(),
            e
        ))
    })?;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue; // Skip empty lines and comments
        }

        if let Some((hosts, key)) = parse_known_host_line(trimmed) {
            for host in hosts {
                trusted_keys.entry(host).or_default().push(key.clone());
            }
        }
    }

    Ok(trusted_keys)
}

/// A high-level wrapper that implements the `CommandExecutor` and `FileTransfer` traits.
pub struct SshCommandExecutor {
    client: SshClient,
}

impl SshCommandExecutor {
    /// Create a new SSH-based executor by connecting to the remote host.
    pub async fn connect(
        key_path: impl AsRef<Path>,
        username: &str,
        addr: &str,
        timeout: Duration,
    ) -> Result<Self, ExecutorError> {
        // --- Resolve hostname/IP ---
        let resolved_addr = tokio::net::lookup_host(addr)
            .await
            .map_err(|e| {
                ExecutorError::SshError(format!("Failed to resolve host '{}': {}", addr, e))
            })?
            .next() // Take the first resolved address
            .ok_or_else(|| {
                ExecutorError::SshError(format!("No addresses found for host '{}'", addr))
            })?;

        // Use resolved IP address string for key lookup. Could also try hostname.
        // Using IP is generally more reliable if DNS entries change but IP stays same.
        let target_host_str = resolved_addr.ip().to_string();
        tracing::debug!("Resolved target host '{}' to IP: {}", addr, target_host_str);

        // --- Load Known Hosts ---
        let known_hosts_path = dirs::home_dir()
            .map(|home| home.join(".ssh").join("known_hosts"))
            .ok_or_else(|| {
                ExecutorError::SshError(
                    "Could not determine home directory for known_hosts file.".to_string(),
                )
            })?;

        tracing::debug!("Loading known hosts from: {}", known_hosts_path.display());
        let known_hosts_map = load_known_hosts(&known_hosts_path).await?;
        let client = SshClient::connect(
            key_path,
            username,
            resolved_addr,
            target_host_str,
            known_hosts_map,
            timeout,
        )
        .await?;
        Ok(SshCommandExecutor { client })
    }
}

#[async_trait]
impl CommandExecutor for SshCommandExecutor {
    async fn execute_command(&mut self, command: &str) -> Result<CommandResult, ExecutorError> {
        self.client.execute_command_internal(command).await
    }

    async fn close(&mut self) -> Result<(), ExecutorError> {
        self.client.close_internal().await
    }
}

#[async_trait]
impl FileTransfer for SshCommandExecutor {
    async fn upload_file(
        &self,
        local_path: &Path,
        remote_path: &Path,
    ) -> Result<(), ExecutorError> {
        self.client
            .upload_file_internal(local_path, remote_path)
            .await
    }
}
