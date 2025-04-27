# DCD - Docker Compose Deployment Tool

**⚠️ Work In Progress: This tool is under active development. Features and command-line arguments may change.**

DCD (Docker Compose Deployment) is a command-line tool designed to streamline the deployment of Docker Compose based applications to remote servers using SSH. It handles analyzing configuration, synchronizing necessary files, and managing the application lifecycle on the target host.

## Features

- **Remote Deployment**: Deploy applications defined by Docker Compose files to remote servers over SSH.
- **Configuration Analysis**: Parse and analyze Docker Compose and environment files locally to understand project structure, required environment variables, exposed ports, and local file dependencies.
- **File Synchronization**: Automatically synchronize Compose files, environment files, and referenced local files/directories (e.g., volume mounts) to the remote server.
- **Remote Docker Setup**: (Optional/Implicit) Can ensure Docker and Docker Compose are installed on the target.
- **Service Management**: Start, stop, and check the status of services defined in the Compose files on the remote host.
- **Health Checking**: Optionally verify service health after deployment using Docker's health check mechanism.
- **Clean Destruction**: Completely remove deployments, including containers, networks, and optionally volumes.

## Installation

### From Cargo

```bash
cargo install dcd
```

### From Binaries

Download the latest binary for your platform from the [releases page](https://github.com/g1ibby/dcd/releases).

### For Developers

After cloning the repository, set up the Git hooks to ensure code quality:

```bash
chmod +x .githooks/pre-commit
git config core.hooksPath .githooks
```

This will enable pre-commit checks that run `cargo fmt` and `cargo clippy` before each commit.

### Live Development Testing

For easier development and testing, you can use `cargo watch` to automatically rebuild and install `dcd` whenever you save changes to the source code:

```bash
cargo watch -c -x check -s "cargo install --path . --debug"
```

## Usage

The basic command structure is:

```bash
dcd [GLOBAL OPTIONS] <COMMAND> [COMMAND OPTIONS]
```

### Global Options

These options apply to all commands:

```
-f, --file <COMPOSE_FILES>...   Docker compose file(s) to use. Can be specified multiple times.
                                (Defaults based on Docker Compose standard behavior if not provided)
-e, --env-file <ENV_FILES>...   Environment file(s) to use. Can be specified multiple times.
                                (Defaults based on Docker Compose standard behavior if not provided)
-i, --identity <IDENTITY_FILE>  Path to the SSH private key for connecting to the remote host.
                                [default: ~/.ssh/id_rsa] # Adjusted default based on parser
-w, --workdir <REMOTE_DIR>      Remote working directory on the target host where files will be synced
                                and commands executed.
                                (Defaults to ~/.dcd/<project_name>)
-v, --verbose...                Increase message verbosity (-v for debug, -vv for trace).
-V, --version                   Print version information.
-h, --help                      Print help information.
```

### Commands

*   **`analyze`**: Parses local configuration and displays analysis results without connecting to a remote host.
    ```bash
    dcd -f docker-compose.yml -e .env analyze
    ```
*   **`up <TARGET>`**: Deploys or updates the application on the specified remote target. This includes syncing files, ensuring Docker is ready, and running `docker compose up`.
    ```bash
    dcd -f docker-compose.yml up deploy_user@remote.server.com:2222
    dcd up root@192.168.1.100 # Uses default port 22
    ```
    *   `--no-health-check`: Skip verifying service health after deployment.
    *   `--no-progress`: Disable the interactive progress spinner.
*   **`status <TARGET>`**: Checks the status of the deployed services on the remote target using `docker compose ps`.
    ```bash
    dcd status deploy_user@remote.server.com
    ```
    *   `--no-progress`: Disable the interactive progress spinner.
*   **`destroy <TARGET>`**: Stops and removes containers, networks, and optionally volumes associated with the deployment on the remote target.
    ```bash
    dcd destroy deploy_user@remote.server.com
    ```
    *   `--force`: Destroy without confirmation and remove associated volumes.
    *   `--no-progress`: Disable the interactive progress spinner.

```bash
```

## Examples

### Basic Deployment

```bash
# Deploy using default compose/env files to host 192.168.1.100 as user 'deploy'
dcd -i ~/.ssh/deploy_key up deploy@192.168.1.100
```

### Using Multiple Compose Files and Environment Files

```bash
# Specify compose files, env files, and a non-standard SSH port
dcd -i ~/.ssh/id_rsa \
  -f docker-compose.base.yml -f docker-compose.prod.yml \
  -e .env.prod -e .env.secrets \
  up prod_user@app.example.com:2222
```

## GitHub Action

DCD is also available as a GitHub Action for deploying your Docker Compose applications directly from your CI/CD pipelines.

### Usage

```yaml
- name: Deploy with DCD
  uses: g1ibby/dcd-deploy@v1
  with:
    command: up
    target: ${{ secrets.SSH_USER }}@${{ secrets.SSH_HOST }} # Combine user and host
    compose_files: "docker-compose.yml docker-compose.prod.yml"
    env_files: ".env.prod"
    ssh_private_key: ${{ secrets.SSH_PRIVATE_KEY }}
    remote_dir: "/opt/myapp"
    # Optional command-specific flags:
    # no_health_check: true # For 'up' command
    # force: true           # For 'destroy' command
    # no_progress: true     # For 'up', 'status', 'destroy'
```

### Inputs

| Input | Description | Required | Default |
|-------|-------------|----------|---------|
| `command` | Command to execute (`analyze`, `up`, `status`, `destroy`) | Yes | `up` |
| `target` | Remote target in `[user@]host[:port]` format (required for `up`, `status`, `destroy`) | No | - |
| `compose_files` | Space-separated list of Docker Compose files | No | (Docker Compose defaults) |
| `env_files` | Space-separated list of environment files | No | (Docker Compose defaults) |
| `ssh_private_key` | SSH private key content | Yes | - |
| `remote_dir` | Remote working directory | No | `/opt/dcd` |
| `no_health_check` | Skip health check after deployment (for up command) | No | `false` |
| `force` | Force destruction without confirmation (for destroy command) | No | `false` |
| `dcd_version` | Version of dcd to use (tag name or "latest") | No | `latest` |

### Example Workflow

```yaml
name: Deploy to Production

on:
  push:
    branches: [main]

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Deploy to production
        uses: g1ibby/dcd-deploy@v1
        with:
          command: 'up' # Explicitly set command
          target: ${{ secrets.PROD_SSH_USER }}@${{ secrets.PROD_SSH_HOST }}
          compose_files: "docker-compose.yml docker-compose.prod.yml"
          env_files: ".env.prod"
          ssh_private_key: ${{ secrets.PROD_SSH_KEY }}
          remote_dir: "/opt/myapp-production"
```

See the [example workflow](.github/workflows/example-deploy.yml) for a more complete example.


## Releasing

This project uses [`cargo-release`](https://github.com/crate-ci/cargo-release) to manage versioning and tagging, and GitHub Actions to handle the build, GitHub release creation, and publishing to crates.io.

**Prerequisites:**

*   Install `cargo-release`: `cargo install cargo-release`
*   Have push access to the repository.
*   Ensure the `CRATES_IO_TOKEN` secret is configured in the GitHub repository settings for the Actions workflow to publish to crates.io.

**Release Process:**

1.  **Ensure `main` is Up-to-Date:** Make sure your local `main` branch is synchronized with the remote repository and that all changes intended for the release are merged.
2.  **Clean Working Directory:** Ensure `git status` shows a clean working directory.
3.  **Run `cargo release`:** Execute `cargo release` with the desired version bump level (e.g., `patch`, `minor`, `major`). Use the `--execute` flag to perform the actions. It's highly recommended to run without `--execute` first to review the plan.
    *   For a patch release: `cargo release patch --execute --no-publish`
    *   For a minor release: `cargo release minor --execute --no-publish`
    *   For a major release: `cargo release major --execute --no-publish`

    `cargo-release` will:
    *   Update the version in `Cargo.toml`.
    *   Commit the version change.
    *   Create a Git tag (e.g., `vX.Y.Z`).
    *   *(By default, it might try to push and publish - ensure your global or local `cargo-release` config doesn't override this if you strictly want the CI to publish)*.
4.  **Push Changes and Tags:** Manually push the commit and the newly created tag to the `main` branch:
    ```bash
    git push --follow-tags origin main
    ```
5.  **Monitor GitHub Actions:** Pushing the tag will trigger the "Release" workflow in GitHub Actions. This workflow will:
    *   Build release binaries for different targets.
    *   Verify that the tag version matches the `Cargo.toml` version.
    *   Create a GitHub Release, attaching the built binaries.
    *   Publish the crate to crates.io using the `CRATES_IO_TOKEN` secret.
