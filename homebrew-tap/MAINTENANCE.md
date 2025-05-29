# Homebrew Tap Maintenance Guide

This document outlines the maintenance procedures for the DCD Homebrew tap.

## Automated Process

The Homebrew formula is automatically updated when you create a new GitHub release:

1. **Tag a new release** in the format `v1.2.3`
2. **GitHub Actions automatically**:
   - Downloads the pre-built macOS binaries (Intel and ARM)
   - Calculates the SHA256 hashes for both architectures
   - Updates the formula file with version and hashes
   - Commits and pushes the changes

## Manual Intervention (if needed)

### If automation fails

1. Check the GitHub Actions logs in the "Update Homebrew Formula" step
2. Common issues:
   - Network timeouts downloading the tarball
   - Git push conflicts
   - Invalid version format

### Manual formula update

```bash
# 1. Calculate SHA256 hashes for both architectures
VERSION="1.2.3"  # Replace with actual version
wget -O intel.tar.gz "https://github.com/g1ibby/dcd/releases/download/v${VERSION}/dcd-x86_64-apple-darwin.tar.gz"
wget -O arm.tar.gz "https://github.com/g1ibby/dcd/releases/download/v${VERSION}/dcd-aarch64-apple-darwin.tar.gz"

SHA256_INTEL=$(sha256sum intel.tar.gz | cut -d' ' -f1)
SHA256_ARM=$(sha256sum arm.tar.gz | cut -d' ' -f1)

# 2. Generate formula from template
cp homebrew-tap/Formula/dcd.rb.template homebrew-tap/Formula/dcd.rb

# 3. Replace placeholders
sed -i "s/__VERSION__/$VERSION/g" homebrew-tap/Formula/dcd.rb
sed -i "s/__SHA256_INTEL__/$SHA256_INTEL/g" homebrew-tap/Formula/dcd.rb
sed -i "s/__SHA256_ARM__/$SHA256_ARM/g" homebrew-tap/Formula/dcd.rb

# 4. Test the formula
brew install ./homebrew-tap/Formula/dcd.rb
brew test dcd

# 5. Commit and push
git add homebrew-tap/Formula/dcd.rb
git commit -m "Update Homebrew formula to v${VERSION}"
git push
```

**Important:** Never edit `dcd.rb` directly - always edit `dcd.rb.template` for permanent changes.

## Testing

### Local testing
```bash
# Test installation from source
brew install --build-from-source ./homebrew-tap/Formula/dcd.rb

# Test that the binary works
dcd --version

# Clean up
brew uninstall dcd
```

### User testing
```bash
# Add tap and install
brew tap g1ibby/dcd
brew install dcd

# Test functionality
dcd --version
dcd --help

# Clean up
brew uninstall dcd
brew untap g1ibby/dcd
```

## Best Practices

1. **Always test locally** before releasing
2. **Keep the tap in sync** with releases - don't skip versions
3. **Monitor GitHub Actions** for failed automation
4. **Use semantic versioning** for predictable automation
5. **Update dependencies** in formula if Rust toolchain requirements change

## Troubleshooting

### Formula won't install
- Check if Rust is available: `brew install rust`
- Verify SHA256 hash matches the tarball
- Test Cargo build manually: `cargo build --release`

### Users report installation issues
- Check recent GitHub Actions runs
- Verify the formula syntax: `brew audit --new-formula dcd`
- Test in clean environment (Docker or VM)

### Automation stopped working
- Check repository permissions for GitHub Actions
- Verify the workflow file syntax
- Ensure no protected branch rules block automatic commits