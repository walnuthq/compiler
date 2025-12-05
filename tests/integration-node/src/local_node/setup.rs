//! Node installation and bootstrap functionality

use std::{fs, path::Path, process::Command};

use anyhow::{anyhow, Context, Result};

use super::{process::kill_process, sync::read_pid, COORD_DIR, data_dir};

// Version configuration for miden-node
// NOTE: When updating miden-client version in Cargo.toml, update this constant to match
// the compatible miden-node version. Both should typically use the same major.minor version.

/// The exact miden-node version that is compatible with the miden-client version used in tests
const MIDEN_NODE_VERSION: &str = "0.12.2";

/// Relative path to local miden-node binary from the compiler workspace root
const LOCAL_NODE_RELATIVE_PATH: &str = "../miden-node/target/release/miden-node";

/// Returns the path to the miden-node binary to use.
///
/// Priority:
/// 1. Local binary at `../miden-node/target/release/miden-node` if it exists
/// 2. Binary in PATH (from `cargo install` or system)
pub fn get_miden_node_path() -> String {
    // Find the compiler workspace root (this crate is at tests/integration-node)
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .unwrap_or_else(|_| std::env::current_dir().unwrap().to_string_lossy().to_string());
    let workspace_root = Path::new(&manifest_dir)
        .parent() // tests/
        .and_then(|p| p.parent()) // compiler workspace root
        .unwrap_or(Path::new("."));

    let local_node_path = workspace_root.join(LOCAL_NODE_RELATIVE_PATH);

    if local_node_path.exists() {
        eprintln!("[LocalNode] Using local miden-node binary: {}", local_node_path.display());
        local_node_path.to_string_lossy().to_string()
    } else {
        eprintln!("[LocalNode] Local miden-node not found at {}, using PATH", local_node_path.display());
        "miden-node".to_string()
    }
}

/// Manages the lifecycle of a local Miden node instance
pub struct LocalMidenNode;

impl LocalMidenNode {
    /// Install miden-node binary if not already installed
    pub fn ensure_installed() -> Result<()> {
        let node_path = get_miden_node_path();
        let is_local = node_path != "miden-node";

        // If using local binary, skip installation entirely
        if is_local {
            let check = Command::new(&node_path).arg("--version").output();
            if let Ok(output) = check {
                if output.status.success() {
                    let version = String::from_utf8_lossy(&output.stdout);
                    let version_line = version.trim();
                    eprintln!("[LocalNode] Local miden-node version: {}", version_line);

                    // Check if we need to clean bootstrap data due to version change
                    // by comparing stored version with current
                    Self::check_and_clean_if_version_changed(version_line)?;

                    return Ok(());
                }
            }
            return Err(anyhow!("Local miden-node binary exists but failed to run"));
        }

        // Allow skipping version check for local builds
        if std::env::var("MIDEN_NODE_SKIP_VERSION_CHECK").is_ok() {
            eprintln!("Skipping miden-node version check (MIDEN_NODE_SKIP_VERSION_CHECK set)");
            let check = Command::new("miden-node").arg("--version").output();
            if let Ok(output) = check {
                let version = String::from_utf8_lossy(&output.stdout);
                eprintln!("Using miden-node: {}", version.trim());
            }
            return Ok(());
        }

        // Check if miden-node is already installed and get version
        let check = Command::new("miden-node").arg("--version").output();

        let need_install = match check {
            Ok(output) if output.status.success() => {
                let version = String::from_utf8_lossy(&output.stdout);
                let version_line = version.lines().next().unwrap_or("");

                // Check if it's the exact version we need
                if version_line.contains(MIDEN_NODE_VERSION) {
                    eprintln!("miden-node already installed: {version_line}");
                    false
                } else {
                    eprintln!(
                        "Found incompatible miden-node version: {version_line} (need \
                         {MIDEN_NODE_VERSION})"
                    );
                    eprintln!("Uninstalling current version...");

                    // Uninstall the current version
                    let uninstall_output = Command::new("cargo")
                        .args(["uninstall", "miden-node"])
                        .output()
                        .context("Failed to run cargo uninstall")?;

                    if !uninstall_output.status.success() {
                        let stderr = String::from_utf8_lossy(&uninstall_output.stderr);
                        eprintln!("Warning: Failed to uninstall miden-node: {stderr}");
                    } else {
                        eprintln!("Successfully uninstalled old version");
                    }

                    // Clean all node-related data when version changes
                    eprintln!("Cleaning node data due to version change...");

                    // Kill any running node process
                    if let Ok(Some(pid)) = read_pid() {
                        eprintln!("Stopping existing node process {pid}");
                        let _ = kill_process(pid);
                    }

                    // Clean the entire coordination directory
                    if let Err(e) = fs::remove_dir_all(COORD_DIR) {
                        if e.kind() != std::io::ErrorKind::NotFound {
                            eprintln!("Warning: Failed to clean coordination directory: {e}");
                        }
                    }

                    true
                }
            }
            _ => {
                eprintln!("miden-node not found");
                true
            }
        };

        if need_install {
            // Install specific version compatible with miden-client
            eprintln!("Installing miden-node version {MIDEN_NODE_VERSION} from crates.io...");
            let output = Command::new("cargo")
                .args(["install", "miden-node", "--version", MIDEN_NODE_VERSION, "--locked"])
                .output()
                .context("Failed to run cargo install")?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow!("Failed to install miden-node: {stderr}"));
            }

            eprintln!("miden-node {MIDEN_NODE_VERSION} installed successfully");
        }

        Ok(())
    }

    /// Check if the miden-node version has changed and clean bootstrap data if needed
    fn check_and_clean_if_version_changed(current_version: &str) -> Result<()> {
        let version_file = Path::new(COORD_DIR).join("node_version");

        // Read the stored version if it exists
        let stored_version = fs::read_to_string(&version_file).ok();

        if let Some(stored) = stored_version {
            if stored.trim() == current_version {
                // Version matches, no need to clean
                return Ok(());
            }
            eprintln!(
                "[LocalNode] Version changed from '{}' to '{}', cleaning bootstrap data...",
                stored.trim(),
                current_version
            );
        } else {
            eprintln!("[LocalNode] No stored version found, will record current version");
        }

        // Version changed or no stored version - clean the data directory
        let data_path = data_dir();
        if data_path.exists() {
            // Kill any running node process
            if let Ok(Some(pid)) = read_pid() {
                eprintln!("[LocalNode] Stopping existing node process {pid}");
                let _ = kill_process(pid);
            }

            // Remove the data directory (will be re-bootstrapped)
            if let Err(e) = fs::remove_dir_all(&data_path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("[LocalNode] Warning: Failed to clean data directory: {e}");
                }
            }

            // Remove the pid file
            let pid_file = Path::new(COORD_DIR).join("node.pid");
            let _ = fs::remove_file(&pid_file);
        }

        // Ensure the COORD_DIR exists
        fs::create_dir_all(COORD_DIR).context("Failed to create coordination directory")?;

        // Store the current version
        fs::write(&version_file, current_version)
            .context("Failed to write version file")?;

        Ok(())
    }

    /// Bootstrap the node with genesis data
    pub fn bootstrap(data_dir: &Path) -> Result<()> {
        eprintln!("Bootstrapping miden-node...");

        let node_path = get_miden_node_path();
        let output = Command::new(&node_path)
            .args([
                "bundled",
                "bootstrap",
                "--data-directory",
                data_dir.to_str().unwrap(),
                "--accounts-directory",
                data_dir.to_str().unwrap(),
            ])
            .output()
            .context("Failed to run miden-node bootstrap command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Failed to bootstrap node: {stderr}"));
        }

        eprintln!("Node bootstrapped successfully");
        Ok(())
    }
}
