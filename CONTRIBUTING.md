# Contributing to DCD

Thank you for your interest in contributing to DCD! This guide will help you get started with development and understand our processes.

## Development Setup

### Prerequisites

- Rust
- Docker and Docker Compose (for integration tests)
- [Testcontainers](https://testcontainers.com/) (for integration tests)

### Initial Setup

1. **Clone the repository:**
   ```bash
   git clone https://github.com/g1ibby/dcd.git
   cd dcd
   ```

2. **Set up Git hooks for code quality:**
   ```bash
   chmod +x .githooks/pre-commit
   git config core.hooksPath .githooks
   ```
   
   The pre-commit hook automatically runs `cargo fmt`, `cargo clippy`, and `cargo test --all-features` on staged changes in `src/`.

### Development Commands

#### Building and Running
```bash
# Build the project
cargo build

# Run with debug build
cargo run -- <args>

# Install locally for testing
cargo install --path . --debug

# Live development with auto-rebuild and install
cargo watch -c -x check -s "cargo install --path . --debug"
```

#### Testing
```bash
# Run unit tests
cargo test

# Run all tests including integration tests
cargo test --all-features

# Run specific test
cargo test test_name

# Run integration tests only
cargo test --features integration-tests
```

#### Code Quality
```bash
# Check formatting
cargo fmt --all -- --check

# Auto-format code
cargo fmt

# Run clippy linter
cargo clippy --all-targets --all-features -- -D warnings

# Auto-fix clippy issues (if available)
cargo clippy --fix --allow-dirty --allow-staged -- -D warnings
```

## Development Workflow

### Live Development Testing

For easier development and testing, you can use `cargo watch` to automatically rebuild and install `dcd` whenever you save changes:

```bash
cargo watch -c -x check -s "cargo install --path . --debug"
```

This command will:
- Clear the terminal on each run
- Run `cargo check` to verify compilation
- Install the debug version locally so you can test changes immediately

### Code Style Guidelines

- Follow standard Rust formatting (`cargo fmt`)
- Address all clippy warnings
- Write meaningful commit messages
- Add tests for new functionality
- Update documentation when adding features

### Architecture Overview

DCD is organized into four main modules:

- **CLI (`src/cli/`)**: Command-line interface handling
- **Composer (`src/composer/`)**: Docker Compose configuration analysis
- **Deployer (`src/deployer/`)**: Remote deployment orchestration  
- **Executor (`src/executor/`)**: Command execution abstraction


## Testing

### Unit Tests
Unit tests are embedded in modules using `#[cfg(test)]`. Run them with:
```bash
cargo test
```

### Integration Tests
Integration tests are in the `tests/` directory and require Docker and Testcontainers. They use the `integration-tests` feature flag:
```bash
cargo test --features integration-tests
```

### Adding Tests
- Add unit tests for new functions and modules
- Add integration tests for end-to-end scenarios
- Ensure tests are deterministic and don't depend on external state

## Submitting Changes

### Pull Request Process

1. **Fork the repository** and create a feature branch
2. **Make your changes** following the code style guidelines
3. **Add tests** for new functionality
4. **Run the full test suite** to ensure nothing breaks
5. **Submit a pull request** with a clear description of changes

### Commit Message Format

Use clear, descriptive commit messages:
- `feat: add support for custom SSH ports`
- `fix: handle missing environment files gracefully`
- `docs: update installation instructions`
- `test: add integration tests for destroy command`

## Releasing

### Release Process

This project uses [`cargo-release`](https://github.com/crate-ci/cargo-release) to manage versioning and tagging, and GitHub Actions to handle the build, GitHub release creation, and publishing to crates.io.

**Prerequisites:**
- Install `cargo-release`: `cargo install cargo-release`
- Have push access to the repository
- Ensure the `CRATES_IO_TOKEN` secret is configured in GitHub repository settings

**Steps:**

1. **Ensure `main` is Up-to-Date:** Make sure your local `main` branch is synchronized with the remote repository and that all changes intended for the release are merged.

2. **Clean Working Directory:** Ensure `git status` shows a clean working directory.

3. **Run `cargo release`:** Execute `cargo release` with the desired version bump level. Use the `--execute` flag to perform the actions. It's recommended to run without `--execute` first to review the plan.
   
   ```bash
   # For a patch release
   cargo release patch --execute --no-publish
   
   # For a minor release  
   cargo release minor --execute --no-publish
   
   # For a major release
   cargo release major --execute --no-publish
   ```
   
   `cargo-release` will:
   - Update the version in `Cargo.toml`
   - Commit the version change
   - Create a Git tag (e.g., `vX.Y.Z`)

4. **Push Changes and Tags:** Manually push the commit and the newly created tag:
   ```bash
   git push --follow-tags origin main
   ```

5. **Monitor GitHub Actions:** Pushing the tag will trigger the "Release" workflow in GitHub Actions, which will:
   - Build release binaries for different targets
   - Verify that the tag version matches the `Cargo.toml` version
   - Create a GitHub Release with built binaries
   - Publish the crate to crates.io

## Getting Help

- **Issues**: Report bugs and request features via [GitHub Issues](https://github.com/g1ibby/dcd/issues)

We appreciate all contributions, whether they're bug reports, feature requests, documentation improvements, or code changes!
