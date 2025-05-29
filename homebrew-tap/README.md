# DCD Homebrew Tap

This directory contains the Homebrew tap for DCD (Docker Compose Deployment tool).

## For Users

### Installation

```bash
# Add the tap
brew tap g1ibby/dcd

# Install DCD
brew install dcd
```

Or install directly without adding the tap:

```bash
brew install g1ibby/dcd/dcd
```

### Updating

```bash
brew update
brew upgrade dcd
```

## For Maintainers

### How it Works

This tap is automatically maintained by GitHub Actions using a template-based approach:

**Files:**
- `Formula/dcd.rb.template` - Template with placeholders (`__VERSION__`, `__SHA256_INTEL__`, etc.)
- `Formula/dcd.rb` - Generated formula file used by Homebrew

**Process:** When a new release is tagged:

1. The `release.yml` workflow runs
2. Downloads the pre-built macOS binaries (Intel and ARM)
3. Calculates the SHA256 hashes for both architectures
4. Copies template to formula file and replaces placeholders
5. Commits and pushes the updated formula

**Benefits:**
- ⚡ **Fast installation** - no compilation required
- 🔧 **No build dependencies** - users don't need Rust
- 🎯 **Architecture-specific** - optimized for Intel and Apple Silicon

### Testing the Formula

To test the formula before releasing:

```bash
# Local testing (requires existing release)
brew install ./Formula/dcd.rb

# Test basic functionality
brew test dcd

# Or test in a clean environment
brew test-bot --only-formulae dcd
```

### Manual Updates (if needed)

If automation fails, you can manually update the formula:

1. Update version in `Formula/dcd.rb`
2. Update the SHA256 hash
3. Test locally
4. Commit and push

The formula follows standard Homebrew conventions for Rust projects using Cargo.