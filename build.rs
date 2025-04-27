use std::env;
use std::process::Command;

fn main() {
    // Only run if explicitly building for release or if BUILD_VERSION_WITH_HASH is set
    // This avoids potentially slow git lookups during regular debug builds unless requested.
    let profile = env::var("PROFILE").unwrap_or_default();
    let force_hash = env::var("BUILD_VERSION_WITH_HASH").is_ok();

    let mut version_string = env::var("CARGO_PKG_VERSION").unwrap_or_default();

    if profile == "release" || force_hash {
        // Attempt to get the short git hash
        let git_output = Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .output();

        if let Ok(output) = git_output {
            if output.status.success() {
                let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !hash.is_empty() {
                    version_string = format!("{} ({})", version_string, hash);
                }
            } else {
                // Git command failed, maybe not in a git repo or git not installed
                eprintln!(
                    "cargo:warning=Failed to get git hash: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        } else {
            // Command execution failed
            eprintln!("cargo:warning=Failed to execute git command. Is git installed and in PATH?");
        }
    } else {
        // For non-release builds without the flag, add a hint
        version_string = format!("{} (dev)", version_string);
    }

    // Set the final version string as an environment variable for the main crate compilation
    println!("cargo:rustc-env=DCD_BUILD_VERSION={}", version_string);

    // Re-run build script if git HEAD changes (important for hash updates)
    println!("cargo:rerun-if-changed=.git/HEAD");
    // Also consider packed refs for changes
    println!("cargo:rerun-if-changed=.git/packed-refs");
    // Re-run if Cargo.toml changes (version bump)
    println!("cargo:rerun-if-changed=Cargo.toml");
}
