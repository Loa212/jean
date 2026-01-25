//! GitHub CLI management module
//!
//! Handles downloading, installing, and managing the GitHub CLI (gh) binary
//! embedded within the Jean application.

mod commands;
pub(crate) mod config;

pub use commands::*;

use std::path::Path;
use std::process::Command;

// === Cross-platform gh Command Wrapper ===

/// Create a gh command that works on all platforms
/// On Windows with use_wsl=true, uses WSL with the WSL-installed gh binary
/// On Windows with use_wsl=false, uses native Windows gh binary
/// On Unix, uses native gh (use_wsl is ignored)
/// Note: WSL UNC paths (\\wsl.localhost\...) always use WSL regardless of use_wsl setting
#[cfg(windows)]
pub fn create_gh_command(
    args: &[&str],
    working_dir: &Path,
    use_wsl: bool,
) -> Result<Command, String> {
    use crate::platform::shell::{
        escape_for_bash, is_wsl_available, is_wsl_unc_path, parse_wsl_path,
    };

    let working_dir_str = working_dir.to_str().unwrap_or(".");

    // WSL UNC paths always require WSL, regardless of use_wsl preference
    let force_wsl = is_wsl_unc_path(working_dir_str);

    if use_wsl || force_wsl {
        // WSL mode
        if !is_wsl_available() {
            return Err(
                "WSL is required on Windows to run GitHub CLI. \
                 Install WSL with: wsl --install"
                    .to_string(),
            );
        }

        let wsl_gh_path = config::get_wsl_gh_cli_binary_path()?;
        let path_info = parse_wsl_path(working_dir_str);

        // Quote each argument to handle spaces and special characters
        let quoted_args: Vec<String> = args
            .iter()
            .map(|arg| format!("'{}'", escape_for_bash(arg)))
            .collect();
        let args_str = quoted_args.join(" ");

        let cmd_str = format!(
            "cd '{}' && '{}' {args_str}",
            escape_for_bash(&path_info.path),
            escape_for_bash(&wsl_gh_path)
        );

        let mut command = Command::new("wsl");
        // Use specific distribution if available (for WSL UNC paths)
        if let Some(distro) = &path_info.distribution {
            command.args(["-d", distro, "-e", "bash", "-c", &cmd_str]);
        } else {
            command.args(["-e", "bash", "-c", &cmd_str]);
        }
        Ok(command)
    } else {
        // Native Windows mode - use gh from PATH
        let gh_path =
            which::which("gh").map_err(|_| "GitHub CLI not found in PATH".to_string())?;

        let mut command = Command::new(gh_path);
        command.args(args).current_dir(working_dir);
        Ok(command)
    }
}

#[cfg(not(windows))]
pub fn create_gh_command(
    args: &[&str],
    working_dir: &Path,
    _use_wsl: bool,
) -> Result<Command, String> {
    // Get the app handle to find the binary path
    // For non-Windows, we use the locally installed gh binary
    // This requires the app handle, but we can use the global path finder
    let gh_path = which::which("gh").map_err(|_| "GitHub CLI not found in PATH".to_string())?;

    let mut command = Command::new(gh_path);
    command.args(args).current_dir(working_dir);
    Ok(command)
}

/// Create a gh command using the app handle to find the binary
#[cfg(not(windows))]
pub fn create_gh_command_with_app(
    app: &tauri::AppHandle,
    args: &[&str],
    working_dir: &Path,
    _use_wsl: bool,
) -> Result<Command, String> {
    let binary_path = config::get_gh_cli_binary_path(app)?;

    if !binary_path.exists() {
        return Err("GitHub CLI not installed".to_string());
    }

    let mut command = Command::new(&binary_path);
    command.args(args).current_dir(working_dir);
    Ok(command)
}

#[cfg(windows)]
pub fn create_gh_command_with_app(
    _app: &tauri::AppHandle,
    args: &[&str],
    working_dir: &Path,
    use_wsl: bool,
) -> Result<Command, String> {
    // On Windows, we don't need the app handle since we use WSL paths
    create_gh_command(args, working_dir, use_wsl)
}
