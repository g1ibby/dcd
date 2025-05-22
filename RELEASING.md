# Releasing

This project uses [`cargo-release`](https://github.com/crate-ci/cargo-release) to manage versioning and tagging, and GitHub Actions to handle the build, GitHub release creation, and publishing to crates.io.

**Prerequisites:**

* Install `cargo-release`: `cargo install cargo-release`
* Have push access to the repository.
* Ensure the `CRATES_IO_TOKEN` secret is configured in the GitHub repository settings for the Actions workflow to publish to crates.io.

**Release Process:**

1. **Ensure `main` is Up-to-Date:** Make sure your local `main` branch is synchronized with the remote repository and that all changes intended for the release are merged.
2. **Clean Working Directory:** Ensure `git status` shows a clean working directory.
3. **Run `cargo release`:** Execute `cargo release` with the desired version bump level (e.g., `patch`, `minor`, `major`). Use the `--execute` flag to perform the actions. It's highly recommended to run without `--execute` first to review the plan.
   * For a patch release: `cargo release patch --execute --no-publish`
   * For a minor release: `cargo release minor --execute --no-publish`
   * For a major release: `cargo release major --execute --no-publish`

   `cargo-release` will:
   * Update the version in `Cargo.toml`.
   * Commit the version change.
   * Create a Git tag (e.g., `vX.Y.Z`).
   * *(By default, it might try to push and publish - ensure your global or local `cargo-release` config doesn't override this if you strictly want the CI to publish)*.
4. **Push Changes and Tags:** Manually push the commit and the newly created tag to the `main` branch:
   ```bash
   git push --follow-tags origin main
   ```
5. **Monitor GitHub Actions:** Pushing the tag will trigger the "Release" workflow in GitHub Actions. This workflow will:
   * Build release binaries for different targets.
   * Verify that the tag version matches the `Cargo.toml` version.
   * Create a GitHub Release, attaching the built binaries.
   * Publish the crate to crates.io using the `CRATES_IO_TOKEN` secret.