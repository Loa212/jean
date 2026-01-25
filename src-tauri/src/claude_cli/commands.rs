//! Tauri commands for Claude CLI management

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
#[cfg(windows)]
use std::process::{Command, Stdio};
use tauri::{AppHandle, Emitter};

#[cfg(not(windows))]
use super::config::{ensure_cli_dir, get_cli_binary_path};
#[cfg(windows)]
use super::config::{
    ensure_cli_dir, ensure_wsl_cli_dir, get_cli_binary_path, get_wsl_cli_binary_path,
};

/// Extract semver version number from a version string
/// Handles formats like: "1.0.28", "v1.0.28", "Claude CLI 1.0.28"
fn extract_version_number(version_str: &str) -> String {
    // Try to find a semver-like pattern (digits.digits.digits)
    for word in version_str.split_whitespace() {
        let trimmed = word.trim_start_matches('v');
        // Check if it looks like a version number (starts with digit, contains dots)
        if trimmed
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
            && trimmed.contains('.')
        {
            return trimmed.to_string();
        }
    }
    // Fallback: return original string
    version_str.to_string()
}

/// Base URL for Claude CLI binary distribution
const CLAUDE_DIST_BUCKET: &str =
    "https://storage.googleapis.com/claude-code-dist-86c565f3-f756-42ad-8dfa-d59b1c096819/claude-code-releases";

/// Status of the Claude CLI installation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeCliStatus {
    /// Whether Claude CLI is installed
    pub installed: bool,
    /// Installed version (if any)
    pub version: Option<String>,
    /// Path to the CLI binary (if installed)
    pub path: Option<String>,
    /// Mode of operation ("wsl" or "native")
    pub mode: String,
}

/// Information about a Claude CLI release from GitHub
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseInfo {
    /// Version string (e.g., "1.0.0")
    pub version: String,
    /// Git tag name (e.g., "v1.0.0")
    pub tag_name: String,
    /// Publication date in ISO format
    pub published_at: String,
    /// Whether this is a prerelease
    pub prerelease: bool,
}

/// Progress event for CLI installation
#[derive(Debug, Clone, Serialize)]
pub struct InstallProgress {
    /// Current stage of installation
    pub stage: String,
    /// Progress message
    pub message: String,
    /// Percentage complete (0-100)
    pub percent: u8,
}

/// Check if Claude CLI is installed and get its status
#[tauri::command]
pub async fn check_claude_cli_installed(
    app: AppHandle,
) -> Result<ClaudeCliStatus, String> {
    let use_wsl = crate::projects::get_use_wsl_preference(&app).await;
    check_claude_cli_installed_with_mode(&app, use_wsl).await
}

/// Check Claude CLI installation with explicit mode
async fn check_claude_cli_installed_with_mode(
    #[allow(unused_variables)] app: &AppHandle,
    use_wsl: bool,
) -> Result<ClaudeCliStatus, String> {
    log::trace!("Checking Claude CLI installation status (use_wsl={use_wsl})");

    #[cfg(windows)]
    {
        use crate::platform::shell::is_wsl_available;

        if use_wsl {
            // WSL mode
            if !is_wsl_available() {
                return Err(
                    "WSL is required on Windows in WSL mode. \
                     Install WSL with: wsl --install"
                        .to_string(),
                );
            }

            let wsl_path = get_wsl_cli_binary_path()?;
            log::trace!("Checking Claude CLI at WSL path: {}", wsl_path);

            // Check if binary exists in WSL
            let check_cmd = format!("test -x '{wsl_path}' && echo exists");
            let output = Command::new("wsl")
                .args(["-e", "bash", "-c", &check_cmd])
                .output()
                .map_err(|e| format!("Failed to check WSL binary: {e}"))?;

            let exists = String::from_utf8_lossy(&output.stdout)
                .trim()
                .contains("exists");

            if !exists {
                log::trace!("Claude CLI not found in WSL at {}", wsl_path);
                return Ok(ClaudeCliStatus {
                    installed: false,
                    version: None,
                    path: None,
                    mode: "wsl".to_string(),
                });
            }

            // Get version via WSL
            let version = match execute_cli_command_wsl(&wsl_path, "--version") {
                Ok(output) => {
                    if output.status.success() {
                        let version_str =
                            String::from_utf8_lossy(&output.stdout).trim().to_string();
                        log::trace!("Claude CLI raw version output: {}", version_str);
                        let version = extract_version_number(&version_str);
                        log::trace!("Claude CLI parsed version: {}", version);
                        Some(version)
                    } else {
                        log::warn!("Failed to get Claude CLI version");
                        None
                    }
                }
                Err(e) => {
                    log::warn!("Failed to execute Claude CLI: {}", e);
                    None
                }
            };

            return Ok(ClaudeCliStatus {
                installed: true,
                version,
                path: Some(wsl_path),
                mode: "wsl".to_string(),
            });
        } else {
            // Native Windows mode - check app data directory
            let binary_path = get_cli_binary_path(app)?;
            log::trace!("Checking Claude CLI at native path: {:?}", binary_path);

            if !binary_path.exists() {
                log::trace!("Claude CLI not found at {:?}", binary_path);
                return Ok(ClaudeCliStatus {
                    installed: false,
                    version: None,
                    path: None,
                    mode: "native".to_string(),
                });
            }

            // Get version by running claude.exe --version
            let version = match Command::new(&binary_path).arg("--version").output() {
                Ok(output) => {
                    if output.status.success() {
                        let version_str =
                            String::from_utf8_lossy(&output.stdout).trim().to_string();
                        log::trace!("Claude CLI raw version output: {}", version_str);
                        let version = extract_version_number(&version_str);
                        log::trace!("Claude CLI parsed version: {}", version);
                        Some(version)
                    } else {
                        log::warn!("Failed to get Claude CLI version");
                        None
                    }
                }
                Err(e) => {
                    log::warn!("Failed to execute Claude CLI: {}", e);
                    None
                }
            };

            return Ok(ClaudeCliStatus {
                installed: true,
                version,
                path: Some(binary_path.to_string_lossy().to_string()),
                mode: "native".to_string(),
            });
        }
    }

    #[cfg(not(windows))]
    {
        use super::config::get_cli_binary_path;

        // On non-Windows, we need an app handle for the path - use a workaround
        // For now, just check the expected path directly
        let home = std::env::var("HOME").map_err(|_| "HOME not set")?;
        let binary_path = std::path::PathBuf::from(home)
            .join(".local")
            .join("bin")
            .join("claude");

        if !binary_path.exists() {
            log::trace!("Claude CLI not found at {:?}", binary_path);
            return Ok(ClaudeCliStatus {
                installed: false,
                version: None,
                path: None,
                mode: "native".to_string(),
            });
        }

        // Try to get the version by running claude --version
        // Uses shell wrapper on Unix to bypass macOS security restrictions
        let version = match execute_cli_command(&binary_path, "--version") {
            Ok(output) => {
                if output.status.success() {
                    let version_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    log::trace!("Claude CLI raw version output: {}", version_str);
                    let version = extract_version_number(&version_str);
                    log::trace!("Claude CLI parsed version: {}", version);
                    Some(version)
                } else {
                    log::warn!("Failed to get Claude CLI version");
                    None
                }
            }
            Err(e) => {
                log::warn!("Failed to execute Claude CLI: {}", e);
                None
            }
        };

        Ok(ClaudeCliStatus {
            installed: true,
            version,
            path: Some(binary_path.to_string_lossy().to_string()),
            mode: "native".to_string(),
        })
    }
}

/// npm package metadata for version listing
#[derive(Debug, Deserialize)]
struct NpmPackageInfo {
    versions: std::collections::HashMap<String, serde_json::Value>,
    time: std::collections::HashMap<String, String>,
}

/// Platform-specific release information from manifest
#[derive(Debug, Deserialize)]
struct PlatformInfo {
    checksum: String,
}

/// Release manifest containing checksums for all platforms
#[derive(Debug, Deserialize)]
struct Manifest {
    platforms: std::collections::HashMap<String, PlatformInfo>,
}

/// Get available Claude CLI versions from npm registry
#[tauri::command]
pub async fn get_available_cli_versions() -> Result<Vec<ReleaseInfo>, String> {
    log::trace!("Fetching available Claude CLI versions from npm registry");

    let client = reqwest::Client::new();
    let response = client
        .get("https://registry.npmjs.org/@anthropic-ai/claude-code")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch versions: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "npm registry returned status: {}",
            response.status()
        ));
    }

    let package_info: NpmPackageInfo = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse npm response: {e}"))?;

    // Get versions with their publish times
    let mut versions: Vec<ReleaseInfo> = package_info
        .versions
        .keys()
        .map(|version| {
            let published_at = package_info.time.get(version).cloned().unwrap_or_default();
            ReleaseInfo {
                version: version.clone(),
                tag_name: format!("v{version}"),
                published_at,
                prerelease: version.contains('-'), // e.g., 1.0.0-beta
            }
        })
        .collect();

    // Sort by version descending (newest first) using simple string comparison
    // This works for semver since we compare major.minor.patch numerically
    versions.sort_by(|a, b| {
        let a_parts: Vec<u32> = a
            .version
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect();
        let b_parts: Vec<u32> = b
            .version
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect();
        b_parts.cmp(&a_parts)
    });

    // Take only the 5 most recent versions
    versions.truncate(5);

    log::trace!("Found {} Claude CLI versions", versions.len());
    Ok(versions)
}

/// Fetch the latest version string from the distribution bucket
async fn fetch_latest_version() -> Result<String, String> {
    let url = format!("{CLAUDE_DIST_BUCKET}/latest");
    log::trace!("Fetching latest version from {url}");

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch stable version: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to fetch stable version: HTTP {}",
            response.status()
        ));
    }

    let version = response
        .text()
        .await
        .map_err(|e| format!("Failed to read latest version: {e}"))?
        .trim()
        .to_string();

    log::trace!("Latest version: {version}");
    Ok(version)
}

/// Get the platform string for the current system
fn get_platform(use_wsl: bool) -> Result<&'static str, String> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let _ = use_wsl; // Unused on macOS
        return Ok("darwin-arm64");
    }

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        let _ = use_wsl; // Unused on macOS
        return Ok("darwin-x64");
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let _ = use_wsl; // Unused on Linux
        return Ok("linux-x64");
    }

    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        let _ = use_wsl; // Unused on Linux
        return Ok("linux-arm64");
    }

    #[cfg(target_os = "windows")]
    {
        use crate::platform::shell::is_wsl_available;

        if use_wsl {
            // WSL mode - download Linux binary
            if !is_wsl_available() {
                return Err(
                    "WSL is required on Windows in WSL mode. \
                     Install WSL with: wsl --install\n\n\
                     After installation, restart your computer and try again."
                        .to_string(),
                );
            }
            return Ok("linux-x64");
        } else {
            // Native Windows mode - download Windows binary
            return Ok("win32-x64");
        }
    }

    #[allow(unreachable_code)]
    Err("Unsupported platform".to_string())
}

/// Execute Claude CLI command via WSL (Windows only)
/// Takes the WSL path directly (e.g., ~/.local/bin/claude)
#[cfg(windows)]
fn execute_cli_command_wsl(wsl_path: &str, args: &str) -> Result<std::process::Output, String> {
    use crate::platform::shell::wsl_shell_command;

    let cmd = format!("'{wsl_path}' {args}");
    wsl_shell_command(&cmd)?
        .output()
        .map_err(|e| format!("Failed to execute CLI via WSL: {e}"))
}

/// Execute Claude CLI command. Unix only - uses shell wrapper.
#[cfg(not(windows))]
fn execute_cli_command(
    binary_path: &std::path::Path,
    args: &str,
) -> Result<std::process::Output, String> {
    let cmd = format!("{:?} {args}", binary_path);
    crate::platform::shell_command(&cmd)
        .output()
        .map_err(|e| format!("Failed to execute CLI: {e}"))
}

/// Fetch the release manifest containing checksums for all platforms
async fn fetch_manifest(version: &str) -> Result<Manifest, String> {
    let url = format!("{CLAUDE_DIST_BUCKET}/{version}/manifest.json");
    log::trace!("Fetching manifest from {url}");

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch manifest: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to fetch manifest: HTTP {}",
            response.status()
        ));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse manifest: {e}"))
}

/// Verify SHA256 checksum of downloaded data
fn verify_checksum(data: &[u8], expected: &str) -> Result<(), String> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let computed = format!("{:x}", hasher.finalize());

    if computed != expected.to_lowercase() {
        return Err(format!(
            "Checksum mismatch: expected {expected}, got {computed}"
        ));
    }
    Ok(())
}

/// Install binary to WSL via stdin pipe (Windows only)
#[cfg(windows)]
fn install_cli_to_wsl(binary_content: &[u8]) -> Result<String, String> {
    use crate::platform::shell::is_wsl_available;

    if !is_wsl_available() {
        return Err(
            "WSL is required on Windows to install Claude CLI. \
             Install WSL with: wsl --install"
                .to_string(),
        );
    }

    let wsl_path = get_wsl_cli_binary_path()?;
    let wsl_dir = ensure_wsl_cli_dir()?;

    log::trace!("Installing Claude CLI to WSL at: {wsl_path}");
    log::trace!("WSL directory: {wsl_dir}");

    // Install binary via stdin pipe to WSL
    // This writes the binary directly into WSL's native ext4 filesystem
    let install_cmd = format!("cat > '{wsl_path}' && chmod +x '{wsl_path}'");

    let mut child = Command::new("wsl")
        .args(["-e", "bash", "-c", &install_cmd])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn WSL install process: {e}"))?;

    // Write binary content to stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(binary_content)
            .map_err(|e| format!("Failed to write binary to WSL: {e}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for WSL install: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to install CLI to WSL: {stderr}"));
    }

    log::trace!("Claude CLI installed to WSL successfully");
    Ok(wsl_path)
}

/// Install Claude CLI by downloading the binary directly from Anthropic's distribution bucket
#[tauri::command]
pub async fn install_claude_cli(
    app: AppHandle,
    version: Option<String>,
) -> Result<(), String> {
    let use_wsl = crate::projects::get_use_wsl_preference(&app).await;
    log::trace!("Installing Claude CLI, version: {:?}, use_wsl: {}", version, use_wsl);

    // Check if any Claude processes are running - cannot replace binary while in use
    let running_sessions = crate::chat::registry::get_running_sessions();
    if !running_sessions.is_empty() {
        let count = running_sessions.len();
        return Err(format!(
            "Cannot install Claude CLI while {} Claude {} running. Please stop all active sessions first.",
            count,
            if count == 1 { "session is" } else { "sessions are" }
        ));
    }

    // Emit progress: starting
    emit_progress(&app, "starting", "Preparing installation...", 0);

    // Determine version (use provided or fetch stable)
    let version = match version {
        Some(v) => v,
        None => fetch_latest_version().await?,
    };

    // Detect platform (this will error if native Windows mode is selected)
    let platform = get_platform(use_wsl)?;
    log::trace!("Installing version {version} for platform {platform}");

    // Fetch manifest and get expected checksum
    emit_progress(
        &app,
        "fetching_manifest",
        "Fetching release manifest...",
        10,
    );
    let manifest = fetch_manifest(&version).await?;
    let expected_checksum = manifest
        .platforms
        .get(platform)
        .ok_or_else(|| format!("No checksum found for platform {platform}"))?
        .checksum
        .clone();
    log::trace!("Expected checksum for {platform}: {expected_checksum}");

    // Build download URL (Windows binaries have .exe extension)
    let binary_name = if platform == "win32-x64" {
        "claude.exe"
    } else {
        "claude"
    };
    let download_url = format!("{CLAUDE_DIST_BUCKET}/{version}/{platform}/{binary_name}");
    log::trace!("Downloading from: {download_url}");

    // Emit progress: downloading
    emit_progress(&app, "downloading", "Downloading Claude CLI...", 25);

    // Download the binary
    let client = reqwest::Client::new();
    let response = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| format!("Failed to download Claude CLI: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to download Claude CLI: HTTP {}",
            response.status()
        ));
    }

    // Get the binary content
    let binary_content = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read binary content: {e}"))?;

    log::trace!("Downloaded {} bytes", binary_content.len());

    // Verify checksum before writing to disk
    emit_progress(&app, "verifying_checksum", "Verifying checksum...", 55);
    verify_checksum(&binary_content, &expected_checksum)?;
    log::trace!("Checksum verified successfully");

    // Emit progress: installing
    emit_progress(&app, "installing", "Installing Claude CLI...", 65);

    // Platform-specific installation
    #[cfg(windows)]
    {
        if use_wsl {
            // WSL mode - install to WSL filesystem
            let wsl_path = install_cli_to_wsl(&binary_content)?;
            emit_progress(&app, "complete", "Installation complete!", 100);
            log::trace!("Claude CLI installed successfully at {wsl_path}");
        } else {
            // Native Windows mode - install to app data directory
            let _cli_dir = ensure_cli_dir(&app)?;
            let binary_path = get_cli_binary_path(&app)?;

            // Write the binary to the target path
            log::trace!("Creating binary file at {:?}", binary_path);
            let mut file = std::fs::File::create(&binary_path)
                .map_err(|e| format!("Failed to create binary file: {e}"))?;

            log::trace!("Writing {} bytes to binary file", binary_content.len());
            file.write_all(&binary_content)
                .map_err(|e| format!("Failed to write binary file: {e}"))?;
            log::trace!("Binary file written successfully");

            emit_progress(&app, "complete", "Installation complete!", 100);
            log::trace!("Claude CLI installed successfully at {:?}", binary_path);
        }
    }

    #[cfg(not(windows))]
    {
        let _cli_dir = ensure_cli_dir(&app)?;
        let binary_path = get_cli_binary_path(&app)?;

        // Write the binary to the target path
        log::trace!("Creating binary file at {:?}", binary_path);
        let mut file = std::fs::File::create(&binary_path)
            .map_err(|e| format!("Failed to create binary file: {e}"))?;

        log::trace!("Writing {} bytes to binary file", binary_content.len());
        file.write_all(&binary_content)
            .map_err(|e| format!("Failed to write binary file: {e}"))?;
        log::trace!("Binary file written successfully");

        // Make sure the binary is executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            log::trace!(
                "Setting executable permissions (0o755) on {:?}",
                binary_path
            );
            let mut perms = std::fs::metadata(&binary_path)
                .map_err(|e| format!("Failed to get binary metadata: {e}"))?
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&binary_path, perms)
                .map_err(|e| format!("Failed to set binary permissions: {e}"))?;
            log::trace!("Executable permissions set successfully");
        }

        // Remove macOS quarantine attribute to allow execution
        #[cfg(target_os = "macos")]
        {
            use std::process::Command;
            log::trace!("Removing quarantine attribute from {:?}", binary_path);
            let _ = Command::new("xattr")
                .args(["-d", "com.apple.quarantine"])
                .arg(&binary_path)
                .output();
            // Ignore errors - attribute might not exist
        }

        emit_progress(&app, "complete", "Installation complete!", 100);
        log::trace!("Claude CLI installed successfully at {:?}", binary_path);
    }

    Ok(())
}

/// Result of checking Claude CLI authentication status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeAuthStatus {
    /// Whether the CLI is authenticated (can execute queries)
    pub authenticated: bool,
    /// Error message if authentication check failed
    pub error: Option<String>,
}

/// Check if Claude CLI is authenticated by running a simple query
#[tauri::command]
pub async fn check_claude_cli_auth(
    app: AppHandle,
) -> Result<ClaudeAuthStatus, String> {
    let use_wsl = crate::projects::get_use_wsl_preference(&app).await;
    log::trace!("Checking Claude CLI authentication status (use_wsl={use_wsl})");

    // Run a simple non-interactive query to check if authenticated
    // Use --print to avoid interactive mode, and a simple prompt
    let args = "--print --output-format text -p 'Reply with just the word OK'";
    log::trace!("Running auth check with args: {}", args);

    #[cfg(windows)]
    {
        use crate::platform::shell::is_wsl_available;

        if use_wsl {
            // WSL mode
            if !is_wsl_available() {
                return Err("WSL is required on Windows in WSL mode".to_string());
            }

            let wsl_path = get_wsl_cli_binary_path()?;

            // Check if binary exists
            let check_cmd = format!("test -x '{wsl_path}' && echo exists");
            let check_output = Command::new("wsl")
                .args(["-e", "bash", "-c", &check_cmd])
                .output()
                .map_err(|e| format!("Failed to check WSL binary: {e}"))?;

            if !String::from_utf8_lossy(&check_output.stdout)
                .trim()
                .contains("exists")
            {
                return Ok(ClaudeAuthStatus {
                    authenticated: false,
                    error: Some("Claude CLI not installed".to_string()),
                });
            }

            let output = execute_cli_command_wsl(&wsl_path, args)?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                log::trace!("Claude CLI auth check successful, response: {}", stdout);
                Ok(ClaudeAuthStatus {
                    authenticated: true,
                    error: None,
                })
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                log::warn!("Claude CLI auth check failed: {}", stderr);
                Ok(ClaudeAuthStatus {
                    authenticated: false,
                    error: Some(stderr),
                })
            }
        } else {
            // Native Windows mode
            let binary_path = get_cli_binary_path(&app)?;

            if !binary_path.exists() {
                return Ok(ClaudeAuthStatus {
                    authenticated: false,
                    error: Some("Claude CLI not installed".to_string()),
                });
            }

            // Build args as array for native execution
            let output = Command::new(&binary_path)
                .args([
                    "--print",
                    "--output-format",
                    "text",
                    "-p",
                    "Reply with just the word OK",
                ])
                .output()
                .map_err(|e| format!("Failed to execute Claude CLI: {e}"))?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                log::trace!("Claude CLI auth check successful, response: {}", stdout);
                Ok(ClaudeAuthStatus {
                    authenticated: true,
                    error: None,
                })
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                log::warn!("Claude CLI auth check failed: {}", stderr);
                Ok(ClaudeAuthStatus {
                    authenticated: false,
                    error: Some(stderr),
                })
            }
        }
    }

    #[cfg(not(windows))]
    {
        let _ = use_wsl; // Unused on non-Windows
        use super::config::get_cli_binary_path;

        let binary_path = get_cli_binary_path(&app)?;

        if !binary_path.exists() {
            return Ok(ClaudeAuthStatus {
                authenticated: false,
                error: Some("Claude CLI not installed".to_string()),
            });
        }

        let output = execute_cli_command(&binary_path, args)?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            log::trace!("Claude CLI auth check successful, response: {}", stdout);
            Ok(ClaudeAuthStatus {
                authenticated: true,
                error: None,
            })
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            log::warn!("Claude CLI auth check failed: {}", stderr);
            Ok(ClaudeAuthStatus {
                authenticated: false,
                error: Some(stderr),
            })
        }
    }
}

/// Helper function to emit installation progress events
fn emit_progress(app: &AppHandle, stage: &str, message: &str, percent: u8) {
    let progress = InstallProgress {
        stage: stage.to_string(),
        message: message.to_string(),
        percent,
    };

    if let Err(e) = app.emit("claude-cli:install-progress", &progress) {
        log::warn!("Failed to emit install progress: {}", e);
    }
}
