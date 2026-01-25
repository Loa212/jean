// Cross-platform shell detection and command execution

use std::env;
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
use std::sync::OnceLock;

/// Returns the user's default shell path
/// - Unix: Uses $SHELL env var, falls back to /bin/sh
/// - Windows: Returns powershell.exe (for general shell tasks)
#[cfg(unix)]
pub fn get_default_shell() -> String {
    env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
}

#[cfg(windows)]
pub fn get_default_shell() -> String {
    "powershell.exe".to_string()
}

/// Returns shell and arguments for executing a command string
/// - Unix: (shell, ["-c", cmd])
/// - Windows: (powershell, ["-Command", cmd])
#[cfg(unix)]
pub fn get_shell_command_args(cmd: &str) -> (String, Vec<String>) {
    let shell = get_default_shell();
    (shell, vec!["-c".to_string(), cmd.to_string()])
}

#[cfg(windows)]
pub fn get_shell_command_args(cmd: &str) -> (String, Vec<String>) {
    (
        "powershell.exe".to_string(),
        vec!["-Command".to_string(), cmd.to_string()],
    )
}

/// Creates a Command configured to run a shell command string
pub fn shell_command(cmd: &str) -> Command {
    let (shell, args) = get_shell_command_args(cmd);
    let mut command = Command::new(shell);
    command.args(args);
    command
}

/// Returns shell arguments for a login shell (interactive, sources profile)
/// - Unix: Returns ["-l", "-i", "-c", cmd] for login interactive shell
/// - Windows: Returns standard PowerShell args (no login shell concept)
#[allow(dead_code)]
#[cfg(unix)]
pub fn get_login_shell_args(cmd: &str) -> (String, Vec<String>) {
    let shell = get_default_shell();
    (
        shell,
        vec![
            "-l".to_string(),
            "-i".to_string(),
            "-c".to_string(),
            cmd.to_string(),
        ],
    )
}

#[allow(dead_code)]
#[cfg(windows)]
pub fn get_login_shell_args(cmd: &str) -> (String, Vec<String>) {
    // Windows doesn't have login shell concept, use regular PowerShell
    get_shell_command_args(cmd)
}

/// Check if an executable exists in PATH
#[cfg(target_os = "linux")]
pub fn executable_exists(name: &str) -> bool {
    which::which(name).is_ok()
}

#[cfg(not(target_os = "linux"))]
#[allow(dead_code)]
pub fn executable_exists(name: &str) -> bool {
    which::which(name).is_ok()
}

/// Get the path to an executable if it exists
#[allow(dead_code)]
pub fn find_executable(name: &str) -> Option<std::path::PathBuf> {
    which::which(name).ok()
}

// === WSL Support (Windows only) ===

/// Windows flag to prevent cmd window from appearing
#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Create a WSL command with CREATE_NO_WINDOW flag to prevent visible cmd windows
#[cfg(windows)]
pub fn wsl_command() -> Command {
    let mut cmd = Command::new("wsl");
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Create a WSL command (no-op on non-Windows, returns regular wsl command)
#[cfg(not(windows))]
#[allow(dead_code)]
pub fn wsl_command() -> Command {
    Command::new("wsl")
}

/// Check if a path is a WSL UNC path (\\wsl.localhost\... or \\wsl$\...)
#[cfg(windows)]
pub fn is_wsl_unc_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    normalized.starts_with("//wsl.localhost/") || normalized.starts_with("//wsl$/")
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn is_wsl_unc_path(_path: &str) -> bool {
    false
}

/// Check if a path exists, using WSL for UNC paths
#[cfg(windows)]
pub fn path_exists(path: &str) -> Result<bool, String> {
    if is_wsl_unc_path(path) {
        let wsl_path = windows_to_wsl_path(path);
        // Use bash -c with proper quoting for reliable path checking
        let cmd = format!("test -e '{wsl_path}'");
        let output = wsl_command()
            .args(["bash", "-c", &cmd])
            .output()
            .map_err(|e| format!("Failed to check path via WSL: {e}"))?;
        Ok(output.status.success())
    } else {
        Ok(std::path::Path::new(path).exists())
    }
}

#[cfg(not(windows))]
pub fn path_exists(path: &str) -> Result<bool, String> {
    Ok(std::path::Path::new(path).exists())
}

/// Check if a path is a directory, using WSL for UNC paths
#[cfg(windows)]
pub fn path_is_dir(path: &str) -> Result<bool, String> {
    if is_wsl_unc_path(path) {
        let wsl_path = windows_to_wsl_path(path);
        let cmd = format!("test -d '{wsl_path}'");
        let output = wsl_command()
            .args(["bash", "-c", &cmd])
            .output()
            .map_err(|e| format!("Failed to check directory via WSL: {e}"))?;
        Ok(output.status.success())
    } else {
        Ok(std::path::Path::new(path).is_dir())
    }
}

#[cfg(not(windows))]
pub fn path_is_dir(path: &str) -> Result<bool, String> {
    Ok(std::path::Path::new(path).is_dir())
}

/// Check if .git exists in path (file or directory), using WSL for UNC paths
#[cfg(windows)]
pub fn git_dir_exists(path: &str) -> Result<bool, String> {
    if is_wsl_unc_path(path) {
        let wsl_path = windows_to_wsl_path(path);
        let cmd = format!("test -e '{wsl_path}/.git'");
        let output = wsl_command()
            .args(["bash", "-c", &cmd])
            .output()
            .map_err(|e| format!("Failed to check .git via WSL: {e}"))?;
        Ok(output.status.success())
    } else {
        Ok(std::path::Path::new(path).join(".git").exists())
    }
}

#[cfg(not(windows))]
pub fn git_dir_exists(path: &str) -> Result<bool, String> {
    Ok(std::path::Path::new(path).join(".git").exists())
}

/// Cache for WSL availability check (only check once per app lifetime)
#[cfg(windows)]
static WSL_AVAILABLE: OnceLock<bool> = OnceLock::new();

/// Check if WSL is available on Windows
/// Result is cached to avoid spawning multiple processes and visible cmd windows
#[cfg(windows)]
pub fn is_wsl_available() -> bool {
    *WSL_AVAILABLE.get_or_init(|| {
        wsl_command()
            .arg("--status")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn is_wsl_available() -> bool {
    false
}

/// Result of parsing a Windows path for WSL execution
#[cfg(windows)]
pub struct WslPathInfo {
    /// The Linux path inside WSL
    pub path: String,
    /// The distribution name if this is a WSL UNC path, None for Windows drive paths
    pub distribution: Option<String>,
}

/// Parse a Windows path and extract WSL path info
/// Returns the Linux path and optionally the distribution name for UNC paths
#[cfg(windows)]
pub fn parse_wsl_path(win_path: &str) -> WslPathInfo {
    let path = win_path.replace('\\', "/");

    // Handle WSL UNC paths: \\wsl.localhost\Distro\path or \\wsl$\Distro\path
    if path.starts_with("//wsl.localhost/") || path.starts_with("//wsl$/") {
        let parts: Vec<&str> = path.splitn(4, '/').collect();
        // parts = ["", "", "wsl.localhost", "Ubuntu/root/workspace/..."]
        if parts.len() >= 4 {
            if let Some(slash_pos) = parts[3].find('/') {
                let distro = parts[3][..slash_pos].to_string();
                let linux_path = format!("/{}", &parts[3][slash_pos + 1..]);
                return WslPathInfo {
                    path: linux_path,
                    distribution: Some(distro),
                };
            }
        }
        // Fallback if parsing fails
        return WslPathInfo {
            path,
            distribution: None,
        };
    }

    // Handle standard Windows drive paths: C:\path -> /mnt/c/path
    if path.len() >= 2 && path.chars().nth(1) == Some(':') {
        let drive = path.chars().next().unwrap().to_ascii_lowercase();
        return WslPathInfo {
            path: format!("/mnt/{}{}", drive, &path[2..]),
            distribution: None,
        };
    }

    WslPathInfo {
        path,
        distribution: None,
    }
}

/// Convert a Windows path to WSL path format
/// C:\Users\foo\file.txt -> /mnt/c/Users/foo/file.txt
/// \\wsl.localhost\Ubuntu\root\workspace -> /root/workspace
/// \\wsl$\Ubuntu\home\user -> /home/user
#[cfg(windows)]
pub fn windows_to_wsl_path(win_path: &str) -> String {
    let path = win_path.replace('\\', "/");

    // Handle WSL UNC paths: \\wsl.localhost\Distro\path or \\wsl$\Distro\path
    // These are direct access to WSL filesystem from Windows
    // \\wsl.localhost\Ubuntu\root\workspace -> /root/workspace
    // \\wsl$\Ubuntu\home\user -> /home/user
    if path.starts_with("//wsl.localhost/") || path.starts_with("//wsl$/") {
        // Find the distro name (e.g., "Ubuntu") and extract the path after it
        let parts: Vec<&str> = path.splitn(4, '/').collect();
        // parts = ["", "", "wsl.localhost", "Ubuntu/root/workspace/..."]
        //    or = ["", "", "wsl$", "Ubuntu/root/workspace/..."]
        if parts.len() >= 4 {
            // parts[3] = "Ubuntu/root/workspace/..."
            // Find the first slash after distro name to get the Linux path
            if let Some(slash_pos) = parts[3].find('/') {
                return format!("/{}", &parts[3][slash_pos + 1..]);
            }
        }
        // If we can't parse it, return as-is (will likely fail but with clear error)
        return path;
    }

    // Handle standard Windows drive paths: C:\path -> /mnt/c/path
    if path.len() >= 2 && path.chars().nth(1) == Some(':') {
        let drive = path.chars().next().unwrap().to_ascii_lowercase();
        format!("/mnt/{}{}", drive, &path[2..])
    } else {
        path
    }
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn windows_to_wsl_path(path: &str) -> String {
    // On non-Windows, just return the path as-is
    path.to_string()
}

/// Create a Command that runs through WSL (Windows only)
/// Falls back to regular shell command on other platforms
#[cfg(windows)]
pub fn wsl_shell_command(cmd: &str) -> Result<Command, String> {
    if !is_wsl_available() {
        return Err("WSL is required on Windows. Install with: wsl --install".to_string());
    }

    let mut command = wsl_command();
    command.args(["-e", "bash", "-c", cmd]);
    Ok(command)
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn wsl_shell_command(cmd: &str) -> Result<Command, String> {
    // On Unix, just use regular shell
    Ok(shell_command(cmd))
}

// === Cross-platform Git Command Wrapper ===

use std::path::Path;

/// Escape a string for use in a bash single-quoted context
/// Single quotes in the string are escaped as: '\''
pub fn escape_for_bash(s: &str) -> String {
    s.replace('\'', "'\\''")
}

/// Tauri command to check if WSL is available on Windows
#[tauri::command]
pub fn check_wsl_available() -> bool {
    #[cfg(windows)]
    {
        is_wsl_available()
    }
    #[cfg(not(windows))]
    {
        false // Not applicable on non-Windows
    }
}

/// Create a git command that works on all platforms
/// On Windows with use_wsl=true, uses WSL to execute git
/// On Windows with use_wsl=false, uses native Windows git
/// On Unix, uses native git (use_wsl is ignored)
/// Note: WSL UNC paths (\\wsl.localhost\...) always use WSL regardless of use_wsl setting
#[cfg(windows)]
pub fn create_git_command(
    args: &[&str],
    working_dir: &Path,
    use_wsl: bool,
) -> Result<Command, String> {
    let working_dir_str = working_dir.to_str().unwrap_or(".");

    // WSL UNC paths always require WSL, regardless of use_wsl preference
    let force_wsl = is_wsl_unc_path(working_dir_str);

    if use_wsl || force_wsl {
        // WSL mode
        if !is_wsl_available() {
            return Err(
                "WSL is required on Windows to run git commands. \
                 Install WSL with: wsl --install"
                    .to_string(),
            );
        }

        let path_info = parse_wsl_path(working_dir_str);

        // Quote each argument to handle spaces and special characters
        let quoted_args: Vec<String> = args
            .iter()
            .map(|arg| format!("'{}'", escape_for_bash(arg)))
            .collect();
        let args_str = quoted_args.join(" ");

        let cmd_str = format!(
            "cd '{}' && git {args_str}",
            escape_for_bash(&path_info.path)
        );

        let mut command = wsl_command();
        // Use specific distribution if available (for WSL UNC paths)
        if let Some(distro) = &path_info.distribution {
            command.args(["-d", distro, "-e", "bash", "-c", &cmd_str]);
        } else {
            command.args(["-e", "bash", "-c", &cmd_str]);
        }
        Ok(command)
    } else {
        // Native Windows mode
        let mut command = Command::new("git");
        command.args(args).current_dir(working_dir);
        Ok(command)
    }
}

#[cfg(not(windows))]
pub fn create_git_command(
    args: &[&str],
    working_dir: &Path,
    _use_wsl: bool,
) -> Result<Command, String> {
    let mut command = Command::new("git");
    command.args(args).current_dir(working_dir);
    Ok(command)
}

/// Create a git command with environment variables
/// On Windows with use_wsl=true, uses WSL to execute git
/// On Windows with use_wsl=false, uses native Windows git
/// On Unix, uses native git (use_wsl is ignored)
/// Note: WSL UNC paths (\\wsl.localhost\...) always use WSL regardless of use_wsl setting
#[cfg(windows)]
pub fn create_git_command_with_env(
    args: &[&str],
    working_dir: &Path,
    env_vars: &[(&str, &str)],
    use_wsl: bool,
) -> Result<Command, String> {
    let working_dir_str = working_dir.to_str().unwrap_or(".");

    // WSL UNC paths always require WSL, regardless of use_wsl preference
    let force_wsl = is_wsl_unc_path(working_dir_str);

    if use_wsl || force_wsl {
        // WSL mode
        if !is_wsl_available() {
            return Err(
                "WSL is required on Windows to run git commands. \
                 Install WSL with: wsl --install"
                    .to_string(),
            );
        }

        let path_info = parse_wsl_path(working_dir_str);

        // Quote each argument to handle spaces and special characters
        let quoted_args: Vec<String> = args
            .iter()
            .map(|arg| format!("'{}'", escape_for_bash(arg)))
            .collect();
        let args_str = quoted_args.join(" ");

        // Build env var prefix with escaped values
        let env_prefix = env_vars
            .iter()
            .map(|(k, v)| format!("{k}='{}'", escape_for_bash(v)))
            .collect::<Vec<_>>()
            .join(" ");

        let escaped_path = escape_for_bash(&path_info.path);
        let cmd_str = if env_prefix.is_empty() {
            format!("cd '{escaped_path}' && git {args_str}")
        } else {
            format!("cd '{escaped_path}' && {env_prefix} git {args_str}")
        };

        let mut command = wsl_command();
        // Use specific distribution if available (for WSL UNC paths)
        if let Some(distro) = &path_info.distribution {
            command.args(["-d", distro, "-e", "bash", "-c", &cmd_str]);
        } else {
            command.args(["-e", "bash", "-c", &cmd_str]);
        }
        Ok(command)
    } else {
        // Native Windows mode
        let mut command = Command::new("git");
        command.args(args).current_dir(working_dir);
        for (key, value) in env_vars {
            command.env(key, value);
        }
        Ok(command)
    }
}

#[cfg(not(windows))]
pub fn create_git_command_with_env(
    args: &[&str],
    working_dir: &Path,
    env_vars: &[(&str, &str)],
    _use_wsl: bool,
) -> Result<Command, String> {
    let mut command = Command::new("git");
    command.args(args).current_dir(working_dir);
    for (key, value) in env_vars {
        command.env(key, value);
    }
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows_to_wsl_path_standard_drive() {
        // Standard Windows drive paths
        assert_eq!(
            windows_to_wsl_path(r"C:\Users\test\project"),
            "/mnt/c/Users/test/project"
        );
        assert_eq!(
            windows_to_wsl_path(r"D:\workspace\projects"),
            "/mnt/d/workspace/projects"
        );
    }

    #[test]
    fn test_windows_to_wsl_path_wsl_localhost() {
        // WSL UNC paths using wsl.localhost
        assert_eq!(
            windows_to_wsl_path(r"\\wsl.localhost\Ubuntu\root\workspace\projects\test"),
            "/root/workspace/projects/test"
        );
        assert_eq!(
            windows_to_wsl_path(r"\\wsl.localhost\Ubuntu\home\user\project"),
            "/home/user/project"
        );
    }

    #[test]
    fn test_windows_to_wsl_path_wsl_dollar() {
        // Older WSL UNC paths using wsl$
        assert_eq!(
            windows_to_wsl_path(r"\\wsl$\Ubuntu\home\user\project"),
            "/home/user/project"
        );
        assert_eq!(
            windows_to_wsl_path(r"\\wsl$\Debian\var\www\html"),
            "/var/www/html"
        );
    }

    #[test]
    fn test_windows_to_wsl_path_different_distros() {
        // Different distro names
        assert_eq!(
            windows_to_wsl_path(r"\\wsl.localhost\Debian\etc\config"),
            "/etc/config"
        );
        assert_eq!(
            windows_to_wsl_path(r"\\wsl.localhost\Ubuntu-22.04\home\dev"),
            "/home/dev"
        );
    }

    #[test]
    fn test_is_wsl_unc_path() {
        // WSL UNC paths
        assert!(is_wsl_unc_path(r"\\wsl.localhost\Ubuntu\root"));
        assert!(is_wsl_unc_path(r"\\wsl.localhost\Ubuntu\home\user\project"));
        assert!(is_wsl_unc_path(r"\\wsl$\Ubuntu\home\user"));
        assert!(is_wsl_unc_path(r"\\wsl$\Debian\var\www"));

        // Non-WSL paths
        assert!(!is_wsl_unc_path(r"C:\Users\test"));
        assert!(!is_wsl_unc_path(r"D:\workspace\projects"));
        assert!(!is_wsl_unc_path("/home/user/project"));
        assert!(!is_wsl_unc_path("./relative/path"));
    }

    #[test]
    #[cfg(windows)]
    fn test_parse_wsl_path_standard_drive() {
        // Standard Windows drive paths - no distribution
        let info = parse_wsl_path(r"C:\Users\test\project");
        assert_eq!(info.path, "/mnt/c/Users/test/project");
        assert!(info.distribution.is_none());

        let info = parse_wsl_path(r"D:\workspace\projects");
        assert_eq!(info.path, "/mnt/d/workspace/projects");
        assert!(info.distribution.is_none());
    }

    #[test]
    #[cfg(windows)]
    fn test_parse_wsl_path_wsl_localhost() {
        // WSL UNC paths using wsl.localhost - includes distribution
        let info = parse_wsl_path(r"\\wsl.localhost\Ubuntu\root\workspace\projects\test");
        assert_eq!(info.path, "/root/workspace/projects/test");
        assert_eq!(info.distribution, Some("Ubuntu".to_string()));

        let info = parse_wsl_path(r"\\wsl.localhost\Ubuntu\home\user\project");
        assert_eq!(info.path, "/home/user/project");
        assert_eq!(info.distribution, Some("Ubuntu".to_string()));
    }

    #[test]
    #[cfg(windows)]
    fn test_parse_wsl_path_wsl_dollar() {
        // Older WSL UNC paths using wsl$ - includes distribution
        let info = parse_wsl_path(r"\\wsl$\Ubuntu\home\user\project");
        assert_eq!(info.path, "/home/user/project");
        assert_eq!(info.distribution, Some("Ubuntu".to_string()));

        let info = parse_wsl_path(r"\\wsl$\Debian\var\www\html");
        assert_eq!(info.path, "/var/www/html");
        assert_eq!(info.distribution, Some("Debian".to_string()));
    }

    #[test]
    #[cfg(windows)]
    fn test_parse_wsl_path_different_distros() {
        // Different distro names
        let info = parse_wsl_path(r"\\wsl.localhost\Debian\etc\config");
        assert_eq!(info.path, "/etc/config");
        assert_eq!(info.distribution, Some("Debian".to_string()));

        let info = parse_wsl_path(r"\\wsl.localhost\Ubuntu-22.04\home\dev");
        assert_eq!(info.path, "/home/dev");
        assert_eq!(info.distribution, Some("Ubuntu-22.04".to_string()));
    }
}
