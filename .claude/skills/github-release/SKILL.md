---
name: github-release
description: Build a Linux amd64 release binary and upload it to the GitHub release page. Use when asked to build, rebuild, or upload release binaries.
disable-model-invocation: true
user-invocable: true
allowed-tools: Bash, Read, Glob, Grep
argument-hint: [version-tag]
---

# GitHub Release

Build a Linux amd64 release binary and upload it to the GitHub release page for tag **$ARGUMENTS** (e.g. `v0.1.0`).

## Steps

1. **Verify the release exists**:
   ```
   gh release view $ARGUMENTS
   ```

2. **Cross-compile for Linux amd64** using `cargo-zigbuild` with the rustup toolchain (Homebrew rustc lacks cross-compilation targets):
   ```
   PATH="/Users/mlv/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$HOME/.cargo/bin:/opt/homebrew/bin:$PATH" \
     cargo zigbuild --release --target x86_64-unknown-linux-musl
   ```

3. **Verify the binary**:
   ```
   file target/x86_64-unknown-linux-musl/release/https_proxy
   ```
   Expect: `ELF 64-bit LSB executable, x86-64, statically linked, stripped`

4. **Zip the binary**:
   ```
   cd target/x86_64-unknown-linux-musl/release
   zip https_proxy-$ARGUMENTS-linux-amd64.zip https_proxy
   ```

5. **Upload to the release** (replace existing asset if present):
   ```
   gh release upload $ARGUMENTS <full-path-to-zip> --clobber
   ```

6. **Confirm upload**:
   ```
   gh release view $ARGUMENTS
   ```

## Notes

- The binary name is `https_proxy` (with underscore), per `[[bin]]` in Cargo.toml.
- The zip asset name uses underscore: `https_proxy-<tag>-linux-amd64.zip`.
- Uses musl for a fully static binary — no glibc dependency on the target host.
- The `--clobber` flag replaces any existing asset with the same name.
