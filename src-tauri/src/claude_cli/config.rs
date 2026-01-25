//! Configuration and path management for the embedded Claude CLI

use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[cfg(windows)]
use std::process::Command;

/// Directory name for storing the Claude CLI binary
pub const CLI_DIR_NAME: &str = "claude-cli";

/// Name of the Claude CLI binary (platform-specific)
#[cfg(not(target_os = "windows"))]
pub const CLI_BINARY_NAME: &str = "claude";

#[cfg(target_os = "windows")]
pub const CLI_BINARY_NAME: &str = "claude.exe";

/// Name of the Claude CLI binary inside WSL (always Linux binary)
#[cfg(windows)]
pub const WSL_CLI_BINARY_NAME: &str = "claude";

/// Get the directory where Claude CLI is installed
///
/// Returns: `~/Library/Application Support/jean/claude-cli/`
pub fn get_cli_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {e}"))?;
    Ok(app_data_dir.join(CLI_DIR_NAME))
}

/// Get the full path to the Claude CLI binary
///
/// Returns: `~/Library/Application Support/jean/claude-cli/claude`
pub fn get_cli_binary_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(get_cli_dir(app)?.join(CLI_BINARY_NAME))
}

/// Ensure the CLI directory exists, creating it if necessary
pub fn ensure_cli_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let cli_dir = get_cli_dir(app)?;
    std::fs::create_dir_all(&cli_dir)
        .map_err(|e| format!("Failed to create CLI directory: {e}"))?;
    Ok(cli_dir)
}

// === WSL Support (Windows only) ===

/// Get the WSL home directory by querying WSL
#[cfg(windows)]
fn get_wsl_home() -> Result<String, String> {
    let output = Command::new("wsl")
        .args(["-e", "bash", "-c", "echo $HOME"])
        .output()
        .map_err(|e| format!("Failed to get WSL home directory: {e}"))?;

    if !output.status.success() {
        return Err("Failed to get WSL home directory".to_string());
    }

    let home = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if home.is_empty() {
        return Err("WSL home directory is empty".to_string());
    }

    Ok(home)
}

/// Get the WSL directory where Jean's Claude CLI should be installed
///
/// Returns: `~/.local/share/jean/claude-cli` (inside WSL's native ext4 filesystem)
///
/// This is Jean's dedicated directory, separate from any native Windows Claude CLI
/// installations. This ensures Jean doesn't interfere with user's existing setup.
#[cfg(windows)]
pub fn get_wsl_cli_dir() -> Result<String, String> {
    let home = get_wsl_home()?;
    Ok(format!("{home}/.local/share/jean/claude-cli"))
}

/// Get the WSL path to Jean's Claude CLI binary
///
/// Returns: `~/.local/share/jean/claude-cli/claude` (inside WSL's native ext4 filesystem)
///
/// This path is inside WSL's native filesystem (ext4), not on the Windows filesystem,
/// which is required because we need to execute a Linux binary inside WSL.
#[cfg(windows)]
pub fn get_wsl_cli_binary_path() -> Result<String, String> {
    let dir = get_wsl_cli_dir()?;
    Ok(format!("{dir}/{WSL_CLI_BINARY_NAME}"))
}

/// Ensure the WSL CLI directory exists
#[cfg(windows)]
pub fn ensure_wsl_cli_dir() -> Result<String, String> {
    let dir = get_wsl_cli_dir()?;
    let output = Command::new("wsl")
        .args(["-e", "mkdir", "-p", &dir])
        .output()
        .map_err(|e| format!("Failed to create WSL CLI directory: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "Failed to create WSL CLI directory: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(dir)
}
