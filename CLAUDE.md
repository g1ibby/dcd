# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

### Build and Installation
```bash
# Build the project
cargo build

# Install DCD locally for development
cargo install --path .

# Build for release
cargo build --release

# Live development with automatic rebuilds
cargo watch -c -x check -s "cargo install --path . --debug"
```

### Testing
```bash
# Run all tests
cargo test --all-features

# Run specific test
cargo test <test_name>
```

### Linting and Formatting
```bash
# Run formatter
cargo fmt

# Run linter
cargo clippy --all-targets --all-features -- -D warnings

# Apply automatic fixes for clippy warnings
cargo clippy --fix --allow-dirty --allow-staged -- -D warnings
```

### Running DCD
```bash
# Analyze Docker Compose configuration
cargo run -- -f docker-compose.yml -e .env analyze

# Deploy to remote server
cargo run -- -f docker-compose.yml up user@remote-server.com

# Check status of deployed services
cargo run -- status user@remote-server.com

# Destroy deployment on remote server
cargo run -- destroy user@remote-server.com
```

## Architecture Overview

DCD (Docker Compose Deployment) is organized into four main modules:

1. **CLI Module** (`src/cli/`): 
   - Handles command-line parsing, user interaction, and workflow orchestration
   - Uses Clap for handling arguments and subcommands
   - Main subcommands: `analyze`, `up`, `status`, and `destroy`

2. **Composer Module** (`src/composer/`):
   - Analyzes Docker Compose configurations and environment files
   - Extracts required environment variables, exposed ports, and volume mount information
   - Detects Docker Compose version and available commands

3. **Deployer Module** (`src/deployer/`):
   - Manages the actual deployment process to remote servers
   - Handles Docker installation verification
   - Synchronizes files between local and remote systems
   - Configures firewalls for exposed ports
   - Deploys and manages Docker Compose services

4. **Executor Module** (`src/executor/`):
   - Provides abstractions for executing commands and transferring files
   - Implements both local and SSH-based execution options
   - Standardizes command output and error handling

The typical workflow is:
1. CLI parses user commands
2. Composer analyzes Docker Compose configuration 
3. Deployer uses analysis results to prepare environment and deploy services
4. Executor handles the actual command execution and file transfers

## Development Guidelines

1. **Git Hooks**:
   - Install the pre-commit hook to ensure code quality: 
   ```bash
   chmod +x .githooks/pre-commit
   git config core.hooksPath .githooks
   ```
   - The hook automatically runs `cargo fmt` and `cargo clippy` before each commit

2. **Release Process**:
   - This project uses `cargo-release` for versioning and tagging
   - Release commands:
     ```bash
     cargo release patch --execute --no-publish  # For patch releases
     cargo release minor --execute --no-publish  # For minor releases
     cargo release major --execute --no-publish  # For major releases
     ```
   - After running `cargo release`, manually push changes with: `git push --follow-tags origin main`
   - GitHub Actions will build binaries, create a GitHub Release, and publish to crates.io