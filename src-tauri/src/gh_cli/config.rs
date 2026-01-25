//! Configuration and path management for the embedded GitHub CLI

use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[cfg(windows)]
use crate::platform::shell::wsl_command;

/// Directory name for storing the GitHub CLI binary
pub const GH_CLI_DIR_NAME: &str = "gh-cli";

/// Name of the GitHub CLI binary
#[cfg(not(target_os = "windows"))]
pub const GH_CLI_BINARY_NAME: &str = "gh";

#[cfg(target_os = "windows")]
pub const GH_CLI_BINARY_NAME: &str = "gh.exe";

/// Get the directory where GitHub CLI is installed
///
/// Returns: `~/Library/Application Support/jean/gh-cli/` (macOS)
///          `~/.local/share/jean/gh-cli/` (Linux)
///          `%APPDATA%/jean/gh-cli/` (Windows)
pub fn get_gh_cli_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {e}"))?;
    Ok(app_data_dir.join(GH_CLI_DIR_NAME))
}

/// Get the full path to the GitHub CLI binary
///
/// Returns: `~/Library/Application Support/jean/gh-cli/gh` (macOS/Linux)
///          `%APPDATA%/jean/gh-cli/gh.exe` (Windows)
pub fn get_gh_cli_binary_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(get_gh_cli_dir(app)?.join(GH_CLI_BINARY_NAME))
}

/// Ensure the CLI directory exists, creating it if necessary
pub fn ensure_gh_cli_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let cli_dir = get_gh_cli_dir(app)?;
    std::fs::create_dir_all(&cli_dir)
        .map_err(|e| format!("Failed to create GitHub CLI directory: {e}"))?;
    Ok(cli_dir)
}

// === WSL Support (Windows only) ===

/// Name of the GitHub CLI binary inside WSL (always Linux binary)
#[cfg(windows)]
pub const WSL_GH_BINARY_NAME: &str = "gh";

/// Get the WSL home directory by querying WSL
#[cfg(windows)]
fn get_wsl_home() -> Result<String, String> {
    let output = wsl_command()
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

/// Get the WSL directory where GitHub CLI should be installed
///
/// Returns: `~/.local/bin` (inside WSL's native ext4 filesystem)
#[cfg(windows)]
pub fn get_wsl_gh_cli_dir() -> Result<String, String> {
    let home = get_wsl_home()?;
    Ok(format!("{home}/.local/bin"))
}

/// Get the WSL path to the GitHub CLI binary
///
/// Returns: `~/.local/bin/gh` (inside WSL's native ext4 filesystem)
#[cfg(windows)]
pub fn get_wsl_gh_cli_binary_path() -> Result<String, String> {
    let dir = get_wsl_gh_cli_dir()?;
    Ok(format!("{dir}/{WSL_GH_BINARY_NAME}"))
}

/// Ensure the WSL GitHub CLI directory exists
#[cfg(windows)]
pub fn ensure_wsl_gh_cli_dir() -> Result<String, String> {
    let dir = get_wsl_gh_cli_dir()?;
    let output = wsl_command()
        .args(["-e", "mkdir", "-p", &dir])
        .output()
        .map_err(|e| format!("Failed to create WSL GitHub CLI directory: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "Failed to create WSL GitHub CLI directory: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(dir)
}
