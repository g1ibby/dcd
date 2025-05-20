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
HOST="${INPUT_HOST}"
PORT="${INPUT_PORT:-22}"
USER="${INPUT_USER:-root}"
SSH_PRIVATE_KEY="${INPUT_SSH_PRIVATE_KEY}"
REMOTE_DIR="${INPUT_REMOTE_DIR:-/opt/dcd}"
NO_HEALTH_CHECK="${INPUT_NO_HEALTH_CHECK:-false}"
FORCE="${INPUT_FORCE:-false}"

# Validate required inputs
if [ -z "$DCD_COMMAND" ]; then
  error "command input is required."
fi
if [ -z "$HOST" ]; then
  error "host input is required."
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

# Validate port is a number
if ! [[ "$PORT" =~ ^[0-9]+$ ]]; then
  error "port must be a number, got: $PORT"
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

# Add common options
ARGS+=("-H" "$HOST")
ARGS+=("--port" "$PORT")
ARGS+=("-u" "$USER")
ARGS+=("-i" "$KEY_FILE") # Use the key file we created
ARGS+=("-w" "$REMOTE_DIR")

# Add the main command
ARGS+=("$DCD_COMMAND")

# Add command-specific flags
if [ "$DCD_COMMAND" == "up" ] && [ "$NO_HEALTH_CHECK" == "true" ]; then
  ARGS+=("--no-health-check")
fi
if [ "$DCD_COMMAND" == "destroy" ] && [ "$FORCE" == "true" ]; then
  ARGS+=("--force")
fi

# --- Execute dcd Command ---
log "Executing command: dcd ${ARGS[*]}"
log "Starting deployment process..."

# Use exec to replace the shell process with dcd
exec /usr/local/bin/dcd "${ARGS[@]}"
