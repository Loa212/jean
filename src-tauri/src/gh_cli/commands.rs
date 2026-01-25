//! Tauri commands for GitHub CLI management

use serde::{Deserialize, Serialize};
use std::io::Write;
#[cfg(windows)]
use crate::platform::shell::wsl_command;
#[cfg(windows)]
use std::process::Stdio;
use tauri::{AppHandle, Emitter};

#[cfg(not(windows))]
use super::config::{ensure_gh_cli_dir, get_gh_cli_binary_path};
#[cfg(windows)]
use super::config::{
    ensure_gh_cli_dir, ensure_wsl_gh_cli_dir, get_gh_cli_binary_path, get_wsl_gh_cli_binary_path,
};

/// GitHub API URL for releases
const GITHUB_RELEASES_API: &str = "https://api.github.com/repos/cli/cli/releases";

/// Status of the GitHub CLI installation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GhCliStatus {
    /// Whether GitHub CLI is installed
    pub installed: bool,
    /// Installed version (if any)
    pub version: Option<String>,
    /// Path to the CLI binary (if installed)
    pub path: Option<String>,
    /// Mode of operation ("wsl" or "native")
    pub mode: String,
}

/// Information about a GitHub CLI release
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GhReleaseInfo {
    /// Version string (e.g., "2.40.0")
    pub version: String,
    /// Git tag name (e.g., "v2.40.0")
    pub tag_name: String,
    /// Publication date in ISO format
    pub published_at: String,
    /// Whether this is a prerelease
    pub prerelease: bool,
}

/// Progress event for CLI installation
#[derive(Debug, Clone, Serialize)]
pub struct GhInstallProgress {
    /// Current stage of installation
    pub stage: String,
    /// Progress message
    pub message: String,
    /// Percentage complete (0-100)
    pub percent: u8,
}

/// GitHub API release response structure
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    published_at: String,
    prerelease: bool,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

/// Execute gh CLI command via WSL (Windows only)
#[cfg(windows)]
fn execute_gh_command_wsl(wsl_path: &str, args: &str) -> Result<std::process::Output, String> {
    use crate::platform::shell::wsl_shell_command;

    let cmd = format!("'{wsl_path}' {args}");
    wsl_shell_command(&cmd)?
        .output()
        .map_err(|e| format!("Failed to execute gh via WSL: {e}"))
}

/// Check if GitHub CLI is installed and get its status
#[tauri::command]
pub async fn check_gh_cli_installed(
    app: AppHandle,
) -> Result<GhCliStatus, String> {
    let use_wsl = crate::projects::get_use_wsl_preference(&app).await;
    check_gh_cli_installed_with_mode(&app, use_wsl).await
}

/// Check GitHub CLI installation with explicit mode
async fn check_gh_cli_installed_with_mode(
    #[allow(unused_variables)] app: &AppHandle,
    use_wsl: bool,
) -> Result<GhCliStatus, String> {
    log::trace!("Checking GitHub CLI installation status (use_wsl={use_wsl})");

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

            let wsl_path = get_wsl_gh_cli_binary_path()?;
            log::trace!("Checking GitHub CLI at WSL path: {}", wsl_path);

            // Check if binary exists in WSL
            let check_cmd = format!("test -x '{wsl_path}' && echo exists");
            let output = wsl_command()
                .args(["-e", "bash", "-c", &check_cmd])
                .output()
                .map_err(|e| format!("Failed to check WSL binary: {e}"))?;

            let exists = String::from_utf8_lossy(&output.stdout)
                .trim()
                .contains("exists");

            if !exists {
                log::trace!("GitHub CLI not found in WSL at {}", wsl_path);
                return Ok(GhCliStatus {
                    installed: false,
                    version: None,
                    path: None,
                    mode: "wsl".to_string(),
                });
            }

            // Get version via WSL
            let version = match execute_gh_command_wsl(&wsl_path, "--version") {
                Ok(output) => {
                    if output.status.success() {
                        let version_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        let version = version_str
                            .split_whitespace()
                            .nth(2)
                            .map(|s| s.to_string())
                            .unwrap_or(version_str);
                        log::trace!("GitHub CLI version: {}", version);
                        Some(version)
                    } else {
                        log::warn!("Failed to get GitHub CLI version");
                        None
                    }
                }
                Err(e) => {
                    log::warn!("Failed to execute GitHub CLI: {}", e);
                    None
                }
            };

            Ok(GhCliStatus {
                installed: true,
                version,
                path: Some(wsl_path),
                mode: "wsl".to_string(),
            })
        } else {
            // Native Windows mode - first check Jean's managed installation path
            let managed_path = get_gh_cli_binary_path(app)?;
            log::trace!("Checking GitHub CLI at managed path: {:?}", managed_path);

            if managed_path.exists() {
                // Get version from managed installation
                let version = match std::process::Command::new(&managed_path)
                    .arg("--version")
                    .output()
                {
                    Ok(output) => {
                        if output.status.success() {
                            let version_str =
                                String::from_utf8_lossy(&output.stdout).trim().to_string();
                            let version = version_str
                                .split_whitespace()
                                .nth(2)
                                .map(|s| s.to_string())
                                .unwrap_or(version_str);
                            log::trace!("GitHub CLI version: {}", version);
                            Some(version)
                        } else {
                            None
                        }
                    }
                    Err(_) => None,
                };

                return Ok(GhCliStatus {
                    installed: true,
                    version,
                    path: Some(managed_path.to_string_lossy().to_string()),
                    mode: "native".to_string(),
                });
            }

            // Fallback: check PATH for system-wide installation
            match which::which("gh") {
                Ok(gh_path) => {
                    log::trace!("Found GitHub CLI in PATH at: {:?}", gh_path);

                    // Get version
                    let version = match std::process::Command::new(&gh_path)
                        .arg("--version")
                        .output()
                    {
                        Ok(output) => {
                            if output.status.success() {
                                let version_str =
                                    String::from_utf8_lossy(&output.stdout).trim().to_string();
                                let version = version_str
                                    .split_whitespace()
                                    .nth(2)
                                    .map(|s| s.to_string())
                                    .unwrap_or(version_str);
                                log::trace!("GitHub CLI version: {}", version);
                                Some(version)
                            } else {
                                None
                            }
                        }
                        Err(_) => None,
                    };

                    Ok(GhCliStatus {
                        installed: true,
                        version,
                        path: Some(gh_path.to_string_lossy().to_string()),
                        mode: "native".to_string(),
                    })
                }
                Err(_) => {
                    log::trace!("GitHub CLI not found in managed path or PATH");
                    Ok(GhCliStatus {
                        installed: false,
                        version: None,
                        path: None,
                        mode: "native".to_string(),
                    })
                }
            }
        }
    }

    #[cfg(not(windows))]
    {
        let _ = use_wsl; // Unused on non-Windows
        let binary_path = get_gh_cli_binary_path(app)?;

        if !binary_path.exists() {
            log::trace!("GitHub CLI not found at {:?}", binary_path);
            return Ok(GhCliStatus {
                installed: false,
                version: None,
                path: None,
                mode: "native".to_string(),
            });
        }

        // Try to get the version by running gh --version
        let shell_cmd = format!("{:?} --version", binary_path);
        let version = match crate::platform::shell_command(&shell_cmd).output() {
            Ok(output) => {
                if output.status.success() {
                    let version_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    let version = version_str
                        .split_whitespace()
                        .nth(2)
                        .map(|s| s.to_string())
                        .unwrap_or(version_str);
                    log::trace!("GitHub CLI version: {}", version);
                    Some(version)
                } else {
                    log::warn!("Failed to get GitHub CLI version");
                    None
                }
            }
            Err(e) => {
                log::warn!("Failed to execute GitHub CLI: {}", e);
                None
            }
        };

        Ok(GhCliStatus {
            installed: true,
            version,
            path: Some(binary_path.to_string_lossy().to_string()),
            mode: "native".to_string(),
        })
    }
}

/// Get available GitHub CLI versions from GitHub releases API
#[tauri::command]
pub async fn get_available_gh_versions() -> Result<Vec<GhReleaseInfo>, String> {
    log::trace!("Fetching available GitHub CLI versions from GitHub API");

    let client = reqwest::Client::builder()
        .user_agent("Jean-App/1.0")
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let response = client
        .get(GITHUB_RELEASES_API)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch releases: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("GitHub API returned status: {}", response.status()));
    }

    let releases: Vec<GitHubRelease> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse GitHub API response: {e}"))?;

    // Convert to our format, filtering to releases with assets for our platform
    let versions: Vec<GhReleaseInfo> = releases
        .into_iter()
        .filter(|r| !r.assets.is_empty())
        .take(5) // Only take 5 most recent
        .map(|r| {
            // Remove 'v' prefix from tag_name for version
            let version = r
                .tag_name
                .strip_prefix('v')
                .unwrap_or(&r.tag_name)
                .to_string();
            GhReleaseInfo {
                version,
                tag_name: r.tag_name,
                published_at: r.published_at,
                prerelease: r.prerelease,
            }
        })
        .collect();

    log::trace!("Found {} GitHub CLI versions", versions.len());
    Ok(versions)
}

/// Get the platform string for the current system (for gh releases)
fn get_gh_platform(use_wsl: bool) -> Result<(&'static str, &'static str), String> {
    // Returns (platform_string, archive_extension)
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let _ = use_wsl; // Unused on macOS
        return Ok(("macOS_arm64", "zip"));
    }

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        let _ = use_wsl; // Unused on macOS
        return Ok(("macOS_amd64", "zip"));
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let _ = use_wsl; // Unused on Linux
        return Ok(("linux_amd64", "tar.gz"));
    }

    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        let _ = use_wsl; // Unused on Linux
        return Ok(("linux_arm64", "tar.gz"));
    }

    #[cfg(target_os = "windows")]
    {
        if use_wsl {
            // WSL mode - download Linux binary for WSL
            use crate::platform::shell::is_wsl_available;
            if !is_wsl_available() {
                return Err(
                    "WSL is required on Windows in WSL mode. \
                     Install WSL with: wsl --install"
                        .to_string(),
                );
            }
            return Ok(("linux_amd64", "tar.gz"));
        } else {
            // Native Windows mode
            return Ok(("windows_amd64", "zip"));
        }
    }

    #[allow(unreachable_code)]
    Err("Unsupported platform".to_string())
}

/// Install binary to WSL via stdin pipe (Windows only)
#[cfg(windows)]
fn install_gh_to_wsl(binary_content: &[u8]) -> Result<String, String> {
    use crate::platform::shell::is_wsl_available;

    if !is_wsl_available() {
        return Err(
            "WSL is required on Windows to install GitHub CLI. \
             Install WSL with: wsl --install"
                .to_string(),
        );
    }

    let wsl_path = get_wsl_gh_cli_binary_path()?;
    let wsl_dir = ensure_wsl_gh_cli_dir()?;

    log::trace!("Installing GitHub CLI to WSL at: {wsl_path}");
    log::trace!("WSL directory: {wsl_dir}");

    // Install binary via stdin pipe to WSL
    let install_cmd = format!("cat > '{wsl_path}' && chmod +x '{wsl_path}'");

    let mut child = wsl_command()
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
        return Err(format!("Failed to install gh to WSL: {stderr}"));
    }

    log::trace!("GitHub CLI installed to WSL successfully");
    Ok(wsl_path)
}

/// Install GitHub CLI by downloading from GitHub releases
#[tauri::command]
pub async fn install_gh_cli(app: AppHandle, version: Option<String>) -> Result<(), String> {
    let use_wsl = crate::projects::get_use_wsl_preference(&app).await;
    log::trace!("Installing GitHub CLI, version: {:?}, use_wsl: {}", version, use_wsl);

    // Check if any Claude processes are running - Claude may use gh for GitHub operations
    let running_sessions = crate::chat::registry::get_running_sessions();
    if !running_sessions.is_empty() {
        let count = running_sessions.len();
        return Err(format!(
            "Cannot install GitHub CLI while {} Claude {} running. Please stop all active sessions first.",
            count,
            if count == 1 { "session is" } else { "sessions are" }
        ));
    }

    // Emit progress: starting
    emit_progress(&app, "starting", "Preparing installation...", 0);

    // Determine version (use provided or fetch latest)
    let version = match version {
        Some(v) => v,
        None => fetch_latest_gh_version().await?,
    };

    // Detect platform
    let (platform, archive_ext) = get_gh_platform(use_wsl)?;
    log::trace!("Installing version {version} for platform {platform}");

    // Build download URL
    // Format: https://github.com/cli/cli/releases/download/v{version}/gh_{version}_{platform}.{ext}
    let archive_name = format!("gh_{version}_{platform}.{archive_ext}");
    let download_url =
        format!("https://github.com/cli/cli/releases/download/v{version}/{archive_name}");
    log::trace!("Downloading from: {download_url}");

    // Emit progress: downloading
    emit_progress(&app, "downloading", "Downloading GitHub CLI...", 20);

    // Download the archive
    let client = reqwest::Client::builder()
        .user_agent("Jean-App/1.0")
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let response = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| format!("Failed to download GitHub CLI: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to download GitHub CLI: HTTP {}",
            response.status()
        ));
    }

    let archive_content = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read archive content: {e}"))?;

    log::trace!("Downloaded {} bytes", archive_content.len());

    // Emit progress: extracting
    emit_progress(&app, "extracting", "Extracting archive...", 40);

    // On Windows, we need a temp directory on Windows filesystem to extract
    // Then we'll copy the binary into WSL
    #[cfg(windows)]
    let temp_base = std::env::temp_dir();
    #[cfg(not(windows))]
    let temp_base = {
        let cli_dir = ensure_gh_cli_dir(&app)?;
        cli_dir
    };

    // Create temp directory for extraction
    let temp_dir = temp_base.join("jean-gh-install-temp");
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("Failed to create temp directory: {e}"))?;

    // Extract the archive (always tar.gz on Windows since we download Linux binary)
    let extracted_binary_path = if archive_ext == "zip" {
        extract_zip(&archive_content, &temp_dir, &version, platform)?
    } else {
        extract_tar_gz(&archive_content, &temp_dir, &version, platform)?
    };

    // Emit progress: installing
    emit_progress(&app, "installing", "Installing GitHub CLI...", 60);

    #[cfg(windows)]
    {
        if use_wsl {
            // WSL mode: Read the extracted binary and install to WSL
            let binary_content = std::fs::read(&extracted_binary_path)
                .map_err(|e| format!("Failed to read extracted binary: {e}"))?;

            // Clean up temp directory
            let _ = std::fs::remove_dir_all(&temp_dir);

            let wsl_path = install_gh_to_wsl(&binary_content)?;

            // Emit progress: verifying
            emit_progress(&app, "verifying", "Verifying installation...", 80);

            // Verify via WSL
            let version_output = execute_gh_command_wsl(&wsl_path, "--version")?;

            if !version_output.status.success() {
                let stderr = String::from_utf8_lossy(&version_output.stderr);
                return Err(format!("GitHub CLI verification failed: {stderr}"));
            }

            let installed_version = String::from_utf8_lossy(&version_output.stdout)
                .trim()
                .to_string();
            log::trace!("Verified GitHub CLI version: {installed_version}");

            emit_progress(&app, "complete", "Installation complete!", 100);
            log::trace!("GitHub CLI installed successfully at {wsl_path}");
        } else {
            // Native Windows mode: Install to Jean's managed directory
            let _cli_dir = ensure_gh_cli_dir(&app)?;
            let binary_path = get_gh_cli_binary_path(&app)?;

            // Copy the extracted binary to the managed path
            std::fs::copy(&extracted_binary_path, &binary_path)
                .map_err(|e| format!("Failed to copy binary: {e}"))?;

            // Clean up temp directory
            let _ = std::fs::remove_dir_all(&temp_dir);

            // Emit progress: verifying
            emit_progress(&app, "verifying", "Verifying installation...", 80);

            // Verify the binary works
            let version_output = std::process::Command::new(&binary_path)
                .arg("--version")
                .output()
                .map_err(|e| format!("Failed to verify GitHub CLI: {e}"))?;

            if !version_output.status.success() {
                let stderr = String::from_utf8_lossy(&version_output.stderr);
                return Err(format!("GitHub CLI verification failed: {stderr}"));
            }

            let installed_version = String::from_utf8_lossy(&version_output.stdout)
                .trim()
                .to_string();
            log::trace!("Verified GitHub CLI version: {installed_version}");

            emit_progress(&app, "complete", "Installation complete!", 100);
            log::trace!("GitHub CLI installed successfully at {:?}", binary_path);
        }
    }

    #[cfg(not(windows))]
    {
        let binary_path = get_gh_cli_binary_path(&app)?;

        // Move binary to final location
        std::fs::copy(&extracted_binary_path, &binary_path)
            .map_err(|e| format!("Failed to copy binary: {e}"))?;

        // Clean up temp directory
        let _ = std::fs::remove_dir_all(&temp_dir);

        // Emit progress: verifying
        emit_progress(&app, "verifying", "Verifying installation...", 80);

        // Make sure the binary is executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&binary_path)
                .map_err(|e| format!("Failed to get binary metadata: {e}"))?
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&binary_path, perms)
                .map_err(|e| format!("Failed to set binary permissions: {e}"))?;
        }

        // Verify the binary works
        let shell_cmd = format!("{:?} --version", binary_path);
        log::trace!("Running via shell: {:?}", shell_cmd);
        let version_output = crate::platform::shell_command(&shell_cmd)
            .output()
            .map_err(|e| format!("Failed to verify GitHub CLI: {e}"))?;

        if !version_output.status.success() {
            let stderr = String::from_utf8_lossy(&version_output.stderr);
            let stdout = String::from_utf8_lossy(&version_output.stdout);
            log::error!(
                "GitHub CLI verification failed - exit code: {:?}, stdout: {}, stderr: {}",
                version_output.status.code(),
                stdout,
                stderr
            );
            return Err(format!(
                "GitHub CLI binary verification failed: {}",
                if !stderr.is_empty() {
                    stderr.to_string()
                } else {
                    "Unknown error".to_string()
                }
            ));
        }

        let installed_version = String::from_utf8_lossy(&version_output.stdout)
            .trim()
            .to_string();
        log::trace!("Verified GitHub CLI version: {installed_version}");

        emit_progress(&app, "complete", "Installation complete!", 100);
        log::trace!("GitHub CLI installed successfully at {:?}", binary_path);
    }

    Ok(())
}

/// Fetch the latest GitHub CLI version from GitHub API
async fn fetch_latest_gh_version() -> Result<String, String> {
    log::trace!("Fetching latest GitHub CLI version");

    let client = reqwest::Client::builder()
        .user_agent("Jean-App/1.0")
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let response = client
        .get(format!("{GITHUB_RELEASES_API}/latest"))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch latest release: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to fetch latest release: HTTP {}",
            response.status()
        ));
    }

    let release: GitHubRelease = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse release info: {e}"))?;

    let version = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name)
        .to_string();
    log::trace!("Latest GitHub CLI version: {version}");
    Ok(version)
}

/// Extract gh binary from a zip archive (macOS, Windows)
fn extract_zip(
    archive_content: &[u8],
    temp_dir: &std::path::Path,
    version: &str,
    platform: &str,
) -> Result<std::path::PathBuf, String> {
    use std::io::Cursor;

    let cursor = Cursor::new(archive_content);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("Failed to open zip archive: {e}"))?;

    // Extract all files
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read zip entry: {e}"))?;

        let outpath = match file.enclosed_name() {
            Some(path) => temp_dir.join(path),
            None => continue,
        };

        if file.is_dir() {
            std::fs::create_dir_all(&outpath)
                .map_err(|e| format!("Failed to create directory: {e}"))?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    std::fs::create_dir_all(p)
                        .map_err(|e| format!("Failed to create parent directory: {e}"))?;
                }
            }
            let mut outfile = std::fs::File::create(&outpath)
                .map_err(|e| format!("Failed to create file: {e}"))?;
            std::io::copy(&mut file, &mut outfile)
                .map_err(|e| format!("Failed to extract file: {e}"))?;
        }
    }

    // The binary is at gh_{version}_{platform}/bin/gh (or gh.exe on Windows)
    #[cfg(not(target_os = "windows"))]
    let binary_name = "gh";
    #[cfg(target_os = "windows")]
    let binary_name = "gh.exe";

    let binary_path = temp_dir
        .join(format!("gh_{version}_{platform}"))
        .join("bin")
        .join(binary_name);

    if !binary_path.exists() {
        return Err(format!("Binary not found in archive at {:?}", binary_path));
    }

    Ok(binary_path)
}

/// Extract gh binary from a tar.gz archive (Linux)
fn extract_tar_gz(
    archive_content: &[u8],
    temp_dir: &std::path::Path,
    version: &str,
    platform: &str,
) -> Result<std::path::PathBuf, String> {
    use flate2::read::GzDecoder;
    use std::io::Cursor;
    use tar::Archive;

    let cursor = Cursor::new(archive_content);
    let decoder = GzDecoder::new(cursor);
    let mut archive = Archive::new(decoder);

    archive
        .unpack(temp_dir)
        .map_err(|e| format!("Failed to extract tar.gz archive: {e}"))?;

    // The binary is at gh_{version}_{platform}/bin/gh
    let binary_path = temp_dir
        .join(format!("gh_{version}_{platform}"))
        .join("bin")
        .join("gh");

    if !binary_path.exists() {
        return Err(format!("Binary not found in archive at {:?}", binary_path));
    }

    Ok(binary_path)
}

/// Result of checking GitHub CLI authentication status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GhAuthStatus {
    /// Whether the CLI is authenticated
    pub authenticated: bool,
    /// Error message if authentication check failed
    pub error: Option<String>,
}

/// Check if GitHub CLI is authenticated by running `gh auth status`
#[tauri::command]
pub async fn check_gh_cli_auth(
    app: AppHandle,
) -> Result<GhAuthStatus, String> {
    let use_wsl = crate::projects::get_use_wsl_preference(&app).await;
    log::trace!("Checking GitHub CLI authentication status (use_wsl={use_wsl})");

    #[cfg(windows)]
    {
        use crate::platform::shell::is_wsl_available;

        if use_wsl {
            // WSL mode
            if !is_wsl_available() {
                return Err("WSL is required on Windows in WSL mode".to_string());
            }

            let wsl_path = get_wsl_gh_cli_binary_path()?;

            // Check if binary exists
            let check_cmd = format!("test -x '{wsl_path}' && echo exists");
            let check_output = wsl_command()
                .args(["-e", "bash", "-c", &check_cmd])
                .output()
                .map_err(|e| format!("Failed to check WSL binary: {e}"))?;

            if !String::from_utf8_lossy(&check_output.stdout)
                .trim()
                .contains("exists")
            {
                return Ok(GhAuthStatus {
                    authenticated: false,
                    error: Some("GitHub CLI not installed".to_string()),
                });
            }

            let output = execute_gh_command_wsl(&wsl_path, "auth status")?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                log::trace!("GitHub CLI auth check successful: {}", stdout);
                Ok(GhAuthStatus {
                    authenticated: true,
                    error: None,
                })
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                log::warn!("GitHub CLI auth check failed: {}", stderr);
                Ok(GhAuthStatus {
                    authenticated: false,
                    error: Some(stderr),
                })
            }
        } else {
            // Native Windows mode - first check Jean's managed installation path
            let managed_path = get_gh_cli_binary_path(&app)?;

            let gh_path = if managed_path.exists() {
                managed_path
            } else {
                // Fallback to PATH
                match which::which("gh") {
                    Ok(path) => path,
                    Err(_) => {
                        return Ok(GhAuthStatus {
                            authenticated: false,
                            error: Some("GitHub CLI not installed".to_string()),
                        });
                    }
                }
            };

            let output = std::process::Command::new(&gh_path)
                .args(["auth", "status"])
                .output()
                .map_err(|e| format!("Failed to execute GitHub CLI: {e}"))?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                log::trace!("GitHub CLI auth check successful: {}", stdout);
                Ok(GhAuthStatus {
                    authenticated: true,
                    error: None,
                })
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                log::warn!("GitHub CLI auth check failed: {}", stderr);
                Ok(GhAuthStatus {
                    authenticated: false,
                    error: Some(stderr),
                })
            }
        }
    }

    #[cfg(not(windows))]
    {
        let _ = use_wsl; // Unused on non-Windows
        let binary_path = get_gh_cli_binary_path(&app)?;

        if !binary_path.exists() {
            return Ok(GhAuthStatus {
                authenticated: false,
                error: Some("GitHub CLI not installed".to_string()),
            });
        }

        // Run gh auth status to check authentication
        let shell_cmd = format!("{:?} auth status", binary_path);

        log::trace!("Running auth check: {:?}", shell_cmd);

        let output = crate::platform::shell_command(&shell_cmd)
            .output()
            .map_err(|e| format!("Failed to execute GitHub CLI: {e}"))?;

        // gh auth status returns exit code 0 if authenticated, non-zero otherwise
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            log::trace!("GitHub CLI auth check successful: {}", stdout);
            Ok(GhAuthStatus {
                authenticated: true,
                error: None,
            })
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            log::warn!("GitHub CLI auth check failed: {}", stderr);
            Ok(GhAuthStatus {
                authenticated: false,
                error: Some(stderr),
            })
        }
    }
}

/// Helper function to emit installation progress events
fn emit_progress(app: &AppHandle, stage: &str, message: &str, percent: u8) {
    let progress = GhInstallProgress {
        stage: stage.to_string(),
        message: message.to_string(),
        percent,
    };

    if let Err(e) = app.emit("gh-cli:install-progress", &progress) {
        log::warn!("Failed to emit install progress: {}", e);
    }
}
