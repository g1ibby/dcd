#!/bin/bash

# Exit immediately if a command exits with a non-zero status.
set -e

# --- Helper Functions ---
log() {
  echo "[dcd-action] $1"
}

error() {
  echo "[dcd-action] ERROR: $1" >&2
  exit 1
}

# --- Input Processing ---
DCD_COMMAND="${INPUT_COMMAND}"
COMPOSE_FILES_STR="${INPUT_COMPOSE_FILES}"
ENV_FILES_STR="${INPUT_ENV_FILES}"
SSH_PRIVATE_KEY="${INPUT_SSH_PRIVATE_KEY}"
REMOTE_DIR="${INPUT_REMOTE_DIR:-/opt/dcd}"
NO_HEALTH_CHECK="${INPUT_NO_HEALTH_CHECK:-false}"
FORCE="${INPUT_FORCE:-false}"
SSH_TARGET="${INPUT_TARGET}"
NO_WARNINGS="${INPUT_NO_WARNINGS:-false}"

# Validate required inputs
if [ -z "$DCD_COMMAND" ]; then
  error "command input is required."
fi
if [[ "$DCD_COMMAND" =~ ^(up|status|destroy)$ ]] && [ -z "$SSH_TARGET" ]; then
  error "target input is required for command '$DCD_COMMAND'. Specify as [user@]host[:port]."
fi
if [ -z "$SSH_PRIVATE_KEY" ]; then
  error "ssh_private_key input is required."
fi

# Validate command value
if [[ ! "$DCD_COMMAND" =~ ^(analyze|up|status|destroy)$ ]]; then
  error "Invalid command: $DCD_COMMAND. Must be one of: analyze, up, status, destroy"
fi

# Validate boolean inputs
if [[ ! "$NO_HEALTH_CHECK" =~ ^(true|false)$ ]]; then
  error "no_health_check must be 'true' or 'false', got: $NO_HEALTH_CHECK"
fi

if [[ ! "$FORCE" =~ ^(true|false)$ ]]; then
  error "force must be 'true' or 'false', got: $FORCE"
fi
if [[ ! "$NO_WARNINGS" =~ ^(true|false)$ ]]; then
  error "no_warnings must be 'true' or 'false', got: $NO_WARNINGS"
fi

# --- SSH Setup ---
log "Setting up SSH..."
SSH_DIR="$HOME/.ssh"
mkdir -p "$SSH_DIR"
chmod 700 "$SSH_DIR"

KEY_FILE="$SSH_DIR/dcd_action_key"
echo "${SSH_PRIVATE_KEY}" >"$KEY_FILE" || error "Failed to write SSH key"
chmod 600 "$KEY_FILE"

# Add SSH config to disable strict host key checking and specify key
cat <<EOF >"$SSH_DIR/config"
Host *
  StrictHostKeyChecking no
  UserKnownHostsFile /dev/null
  IdentityFile $KEY_FILE
  ServerAliveInterval 60
  ServerAliveCountMax 10
EOF
chmod 600 "$SSH_DIR/config"
log "SSH key and config written."

# Verify SSH key is valid
log "Verifying SSH key..."
ssh-keygen -l -f "$KEY_FILE" >/dev/null 2>&1 || error "Invalid SSH key provided"

# --- Build dcd Command Arguments ---
log "Building dcd command..."
ARGS=()

# Add compose files
if [ -n "$COMPOSE_FILES_STR" ]; then
  # Split string by space into an array
  read -r -a compose_files_arr <<<"$COMPOSE_FILES_STR"
  for file in "${compose_files_arr[@]}"; do
    # Check if file exists
    if [ ! -f "$file" ]; then
      log "Warning: Compose file '$file' not found, but will be passed to dcd anyway"
    fi
    ARGS+=("-f" "$file")
  done
else
  # Add default if string was empty but default exists in action.yml
  if [ "$INPUT_COMPOSE_FILES" == "docker-compose.yml" ]; then
    if [ -f "docker-compose.yml" ]; then
      ARGS+=("-f" "docker-compose.yml")
    else
      log "Warning: Default docker-compose.yml not found, but will be passed to dcd anyway"
      ARGS+=("-f" "docker-compose.yml")
    fi
  fi
fi

# Add env files
if [ -n "$ENV_FILES_STR" ]; then
  # Split string by space into an array
  read -r -a env_files_arr <<<"$ENV_FILES_STR"
  for file in "${env_files_arr[@]}"; do
    # Check if file exists
    if [ ! -f "$file" ]; then
      log "Warning: Environment file '$file' not found, but will be passed to dcd anyway"
    fi
    ARGS+=("-e" "$file")
  done
fi

# Add common options (identity file and working directory)
ARGS+=("-i" "$KEY_FILE")
ARGS+=("-w" "$REMOTE_DIR")
  # Add no-warnings flag if requested
  if [ "$NO_WARNINGS" = "true" ]; then
    ARGS+=("--no-warnings")
  fi

# Add the main command
ARGS+=("$DCD_COMMAND")

# Add command-specific flags
if [ "$DCD_COMMAND" == "up" ] && [ "$NO_HEALTH_CHECK" == "true" ]; then
  ARGS+=("--no-health-check")
fi
if [ "$DCD_COMMAND" == "destroy" ] && [ "$FORCE" == "true" ]; then
  ARGS+=("--force")
fi

# Add target argument for remote commands (up, status, destroy)
if [[ "$DCD_COMMAND" == "up" || "$DCD_COMMAND" == "status" || "$DCD_COMMAND" == "destroy" ]]; then
  ARGS+=("$SSH_TARGET")
fi

# --- Execute dcd Command ---
log "Executing command: dcd ${ARGS[*]}"
log "Starting deployment process..."

# Unset internal variables to avoid leaking into dcd environment
unset DCD_COMMAND COMPOSE_FILES_STR ENV_FILES_STR SSH_TARGET SSH_PRIVATE_KEY REMOTE_DIR NO_HEALTH_CHECK FORCE
# Use exec to replace the shell process with dcd
exec /usr/local/bin/dcd "${ARGS[@]}"
