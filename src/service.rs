//! Systemd service installation and removal (Linux only).
//!
//! The [`install_service`] function copies the binary and config to system
//! directories, writes a systemd unit file, and enables/starts the service.
//! [`uninstall_service`] reverses this, preserving the config directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context};

/// Systemd service name used for the unit file and `systemctl` commands.
const SERVICE_NAME: &str = "https_proxy";

/// Install the proxy as a systemd service.
///
/// Performs the following steps:
/// 1. Copies the current binary to `/usr/local/bin/https_proxy`
/// 2. Copies the config file to `/etc/https_proxy/config.yaml`
/// 3. Writes a systemd unit file to `/etc/systemd/system/https_proxy.service`
/// 4. Runs `systemctl daemon-reload`, `enable`, and `restart`
///
/// Requires root privileges.
pub fn install_service(config_path: String) -> anyhow::Result<()> {
    if !cfg!(target_os = "linux") {
        bail!("service installation is only supported on Linux with systemd");
    }

    let exe = std::env::current_exe().context("failed to determine executable path")?;
    let exe = fs::canonicalize(&exe).unwrap_or(exe);

    let config = PathBuf::from(&config_path);
    let config = fs::canonicalize(&config)
        .with_context(|| format!("config file not found: {config_path}"))?;

    // Verify config is valid before installing
    crate::config::Config::load(config.to_str().unwrap_or(&config_path))
        .context("invalid config file")?;

    let install_dir = Path::new("/usr/local/bin");
    let installed_bin = install_dir.join(SERVICE_NAME);

    // Copy binary to /usr/local/bin
    println!("Installing binary to {}", installed_bin.display());
    fs::copy(&exe, &installed_bin)
        .with_context(|| format!("failed to copy binary to {}", installed_bin.display()))?;

    // Copy config to /etc/https_proxy/
    let config_dir = Path::new("/etc/https_proxy");
    fs::create_dir_all(config_dir).context("failed to create /etc/https_proxy/")?;
    let installed_config = config_dir.join("config.yaml");
    println!("Installing config to {}", installed_config.display());
    fs::copy(&config, &installed_config)
        .with_context(|| format!("failed to copy config to {}", installed_config.display()))?;

    // Write systemd unit file
    let unit = generate_unit(&installed_bin, &installed_config);
    let unit_path = PathBuf::from(format!("/etc/systemd/system/{SERVICE_NAME}.service"));
    println!("Writing service unit to {}", unit_path.display());
    fs::write(&unit_path, &unit)
        .with_context(|| format!("failed to write {}", unit_path.display()))?;

    // Reload systemd and enable service
    run_cmd("systemctl", &["daemon-reload"])?;
    run_cmd("systemctl", &["enable", SERVICE_NAME])?;
    run_cmd("systemctl", &["restart", SERVICE_NAME])?;

    println!();
    println!("Service '{SERVICE_NAME}' installed and started.");
    println!();
    println!("Useful commands:");
    println!("  systemctl status {SERVICE_NAME}");
    println!("  journalctl -u {SERVICE_NAME} -f");
    println!("  systemctl restart {SERVICE_NAME}");
    println!("  systemctl stop {SERVICE_NAME}");

    Ok(())
}

/// Uninstall the systemd service.
///
/// Stops and disables the service, removes the binary and unit file, but
/// preserves `/etc/https_proxy/` (user configuration). Requires root.
pub fn uninstall_service() -> anyhow::Result<()> {
    if !cfg!(target_os = "linux") {
        bail!("service uninstallation is only supported on Linux with systemd");
    }

    let unit_path = format!("/etc/systemd/system/{SERVICE_NAME}.service");

    // Stop and disable
    let _ = run_cmd("systemctl", &["stop", SERVICE_NAME]);
    let _ = run_cmd("systemctl", &["disable", SERVICE_NAME]);

    // Remove files
    for path in [&unit_path, &format!("/usr/local/bin/{SERVICE_NAME}")] {
        if Path::new(path).exists() {
            println!("Removing {path}");
            fs::remove_file(path).with_context(|| format!("failed to remove {path}"))?;
        }
    }

    // Keep /etc/https_proxy/ config dir (user data)
    println!("Config directory /etc/https_proxy/ preserved.");

    run_cmd("systemctl", &["daemon-reload"])?;

    println!("Service '{SERVICE_NAME}' uninstalled.");
    Ok(())
}

/// Generate the contents of a systemd unit file.
fn generate_unit(bin_path: &Path, config_path: &Path) -> String {
    format!(
        "\
[Unit]
Description=Stealth HTTPS Forward Proxy
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={bin} run --config {config}
Restart=on-failure
RestartSec=5
AmbientCapabilities=CAP_NET_BIND_SERVICE
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
",
        bin = bin_path.display(),
        config = config_path.display(),
    )
}

/// Run an external command, returning an error if it exits non-zero.
fn run_cmd(program: &str, args: &[&str]) -> anyhow::Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("failed to run {program}"))?;
    if !status.success() {
        bail!("{program} {} failed with {status}", args.join(" "));
    }
    Ok(())
}
