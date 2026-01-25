//! Detached Claude CLI execution
//!
//! This module handles spawning Claude CLI as a fully detached process that
//! survives Jean quitting. The process writes directly to a JSONL file,
//! which Jean tails for real-time updates.

#[cfg(unix)]
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

// Re-export is_process_alive from platform module
pub use crate::platform::is_process_alive;

/// Escape a string for safe use in a shell command.
fn shell_escape(s: &str) -> String {
    // Use single quotes and escape any single quotes within
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Spawn Claude CLI as a detached process that survives Jean quitting (Unix).
///
/// Uses `nohup` and shell backgrounding to fully detach the process.
/// The process reads input from a file and writes output to the NDJSON file.
///
/// Returns the PID of the detached Claude CLI process.
#[cfg(unix)]
#[allow(clippy::too_many_arguments)]
pub fn spawn_detached_claude(
    cli_path: &Path,
    args: &[String],
    input_file: &Path,
    output_file: &Path,
    working_dir: &Path,
    env_vars: &[(&str, &str)],
) -> Result<u32, String> {
    // Build the shell command:
    // cat input.jsonl | nohup /path/to/claude [args] >> output.jsonl 2>&1 & echo $!
    //
    // NOTE: We use `cat file | nohup claude` instead of `nohup claude < file` because
    // Claude CLI with --print doesn't accept stdin from file redirection, only from pipes.
    //
    // - cat: Reads input file and pipes to stdin
    // - nohup: Makes the process immune to SIGHUP (sent when terminal closes)
    // - >> output.jsonl: Appends output to file (Claude writes here)
    // - 2>&1: Redirect stderr to stdout (both go to output file)
    // - &: Run in background
    // - echo $!: Print the PID of the background process

    // Escape ALL paths for safe shell usage (paths may contain spaces like "Application Support")
    let cli_path_escaped =
        shell_escape(cli_path.to_str().ok_or("CLI path contains invalid UTF-8")?);
    let input_path_escaped = shell_escape(
        input_file
            .to_str()
            .ok_or("Input file path contains invalid UTF-8")?,
    );
    let output_path_escaped = shell_escape(
        output_file
            .to_str()
            .ok_or("Output file path contains invalid UTF-8")?,
    );

    // Build args string with proper escaping
    let args_str = args
        .iter()
        .map(|arg| shell_escape(arg))
        .collect::<Vec<_>>()
        .join(" ");

    // Build environment variable exports
    let env_exports = env_vars
        .iter()
        .map(|(k, v)| format!("{}={}", k, shell_escape(v)))
        .collect::<Vec<_>>()
        .join(" ");

    // The full shell command - use cat pipe instead of file redirection
    // Claude CLI with --print requires piped stdin, not file redirection
    // NOTE: env vars must be placed AFTER the pipe so they apply to Claude, not cat
    let shell_cmd = if env_exports.is_empty() {
        format!(
            "cat {input_path_escaped} | nohup {cli_path_escaped} {args_str} >> {output_path_escaped} 2>&1 & echo $!"
        )
    } else {
        format!(
            "cat {input_path_escaped} | {env_exports} nohup {cli_path_escaped} {args_str} >> {output_path_escaped} 2>&1 & echo $!"
        )
    };

    log::trace!("Spawning detached Claude CLI");
    log::trace!("Shell command: {shell_cmd}");
    log::trace!("Working directory: {working_dir:?}");

    // Spawn the shell command
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&shell_cmd)
        .current_dir(working_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn shell: {e}"))?;

    // Read the PID from stdout (the `echo $!` part)
    let stdout = child
        .stdout
        .take()
        .ok_or("Failed to capture shell stdout")?;
    let reader = BufReader::new(stdout);

    let mut pid_str = String::new();
    for line in reader.lines() {
        match line {
            Ok(l) => {
                pid_str = l.trim().to_string();
                break;
            }
            Err(e) => {
                log::warn!("Error reading PID from shell: {e}");
            }
        }
    }

    // Capture stderr for error reporting
    let stderr_handle = child.stderr.take();

    // Wait for shell to finish (it returns immediately after backgrounding)
    let status = child
        .wait()
        .map_err(|e| format!("Failed to wait for shell: {e}"))?;

    if !status.success() {
        // Read stderr to provide better error messages
        let stderr_output = stderr_handle
            .map(|stderr| {
                BufReader::new(stderr)
                    .lines()
                    .map_while(Result::ok)
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();

        return Err(format!(
            "Shell command failed with status: {status}\nStderr: {stderr_output}"
        ));
    }

    // Parse the PID
    let pid: u32 = pid_str
        .parse()
        .map_err(|e| format!("Failed to parse PID '{pid_str}': {e}"))?;

    log::trace!("Detached Claude CLI spawned with PID: {pid}");

    Ok(pid)
}

/// Spawn Claude CLI as a detached process via WSL, using a WSL-native CLI path (Windows only).
///
/// Unlike `spawn_detached_claude`, this function takes a WSL path string directly
/// for the CLI binary (e.g., `/home/user/.local/share/jean/claude-cli/claude`).
/// This is required when the CLI is installed in WSL's native ext4 filesystem.
///
/// The input/output/working_dir paths are still Windows paths and will be converted to WSL paths.
///
/// Returns a placeholder PID (0) because we can't reliably track the PID inside WSL.
/// The caller should use output file content for completion detection.
#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
pub fn spawn_detached_claude_wsl(
    wsl_cli_path: &str, // WSL path like /home/user/.local/share/jean/claude-cli/claude
    args: &[String],
    input_file: &Path,
    output_file: &Path,
    working_dir: &Path,
    env_vars: &[(&str, &str)],
) -> Result<u32, String> {
    use crate::platform::shell::{is_wsl_available, windows_to_wsl_path};
    use std::os::windows::process::CommandExt;

    // Windows process creation flags
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    // Check WSL availability
    if !is_wsl_available() {
        return Err(
            "WSL is required on Windows to run Claude CLI. Install with: wsl --install".to_string(),
        );
    }

    // Convert Windows paths to WSL paths (but NOT the cli_path - it's already WSL)
    let wsl_input_path = windows_to_wsl_path(
        input_file
            .to_str()
            .ok_or("Input file path contains invalid UTF-8")?,
    );
    let wsl_output_path = windows_to_wsl_path(
        output_file
            .to_str()
            .ok_or("Output file path contains invalid UTF-8")?,
    );
    let wsl_working_dir = windows_to_wsl_path(
        working_dir
            .to_str()
            .ok_or("Working directory path contains invalid UTF-8")?,
    );

    // Build args string with proper escaping
    let args_str = args
        .iter()
        .map(|arg| shell_escape(arg))
        .collect::<Vec<_>>()
        .join(" ");

    // Build environment variable exports
    let env_exports = env_vars
        .iter()
        .map(|(k, v)| format!("{}={}", k, shell_escape(v)))
        .collect::<Vec<_>>()
        .join(" ");

    // Shell escape all paths
    let cli_escaped = shell_escape(wsl_cli_path);
    let input_escaped = shell_escape(&wsl_input_path);
    let output_escaped = shell_escape(&wsl_output_path);
    let working_dir_escaped = shell_escape(&wsl_working_dir);

    // Build the shell command to run inside WSL
    // Use nohup BEFORE bash -c for proper process detachment
    let inner_cmd = if env_exports.is_empty() {
        format!(
            "cd {working_dir_escaped} && cat {input_escaped} | {cli_escaped} {args_str} >> {output_escaped} 2>&1"
        )
    } else {
        format!(
            "cd {working_dir_escaped} && cat {input_escaped} | {env_exports} {cli_escaped} {args_str} >> {output_escaped} 2>&1"
        )
    };

    log::trace!("Spawning detached Claude CLI via WSL (native path)");
    log::trace!("WSL CLI path: {wsl_cli_path}");
    log::trace!("WSL shell command: {inner_cmd}");

    // Spawn wsl.exe with nohup to properly detach the process
    // Using `wsl -e nohup bash -c "..."` ensures the process survives wsl.exe exit
    let child = Command::new("wsl")
        .args(["-e", "nohup", "bash", "-c", &inner_cmd])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("Failed to spawn WSL: {e}"))?;

    // The wsl.exe process will exit after spawning nohup, but the Claude process
    // continues running inside WSL. We return 0 as a placeholder PID since we
    // can't reliably track the actual Linux PID from Windows.
    // The caller uses output file content for completion/failure detection.
    let wsl_pid = child.id();
    log::trace!("WSL process spawned with Windows PID: {wsl_pid} (placeholder, actual Claude runs inside WSL)");

    // Return 0 as placeholder - caller relies on output file for status
    Ok(0)
}

/// Spawn Claude CLI as a detached process using native Windows execution (no WSL).
///
/// This function is for Windows native mode where the CLI is installed as a Windows executable
/// (or via npm). It runs directly on Windows without going through WSL.
///
/// Returns a placeholder PID (0) because Windows detached processes don't easily allow
/// tracking the actual PID when using shell execution. The caller should use output file
/// content for completion detection.
#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
pub fn spawn_detached_claude_native(
    cli_path: &Path,
    args: &[String],
    input_file: &Path,
    output_file: &Path,
    working_dir: &Path,
    env_vars: &[(&str, &str)],
) -> Result<u32, String> {
    use std::os::windows::process::CommandExt;

    // Windows process creation flags
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    const DETACHED_PROCESS: u32 = 0x00000008;

    // Build args string - Windows cmd.exe uses different escaping than bash
    // For cmd.exe, we use double quotes and escape inner double quotes
    let args_str = args
        .iter()
        .map(|arg| {
            // If arg contains spaces or special chars, quote it
            if arg.contains(' ')
                || arg.contains('"')
                || arg.contains('&')
                || arg.contains('|')
                || arg.contains('<')
                || arg.contains('>')
            {
                // Escape inner double quotes by doubling them
                format!("\"{}\"", arg.replace('"', "\"\""))
            } else {
                arg.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    // Build environment variable string for cmd.exe
    // Format: set VAR=value && set VAR2=value2 &&
    let env_prefix = if env_vars.is_empty() {
        String::new()
    } else {
        env_vars
            .iter()
            .map(|(k, v)| format!("set {}={}", k, v.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" && ")
            + " && "
    };

    // Get paths as strings
    let cli_path_str = cli_path
        .to_str()
        .ok_or("CLI path contains invalid UTF-8")?;
    let input_path_str = input_file
        .to_str()
        .ok_or("Input file path contains invalid UTF-8")?;
    let output_path_str = output_file
        .to_str()
        .ok_or("Output file path contains invalid UTF-8")?;

    // Build the cmd.exe command:
    // type input.jsonl | claude [args] >> output.jsonl 2>&1
    //
    // NOTE: We use `type file | claude` instead of `claude < file` because
    // Claude CLI with --print requires piped stdin, not file redirection.
    let shell_cmd = format!(
        "{env_prefix}type \"{input_path_str}\" | \"{cli_path_str}\" {args_str} >> \"{output_path_str}\" 2>&1"
    );

    log::trace!("Spawning detached Claude CLI (native Windows)");
    log::trace!("CLI path: {cli_path_str}");
    log::trace!("Shell command: {shell_cmd}");
    log::trace!("Working directory: {working_dir:?}");

    // Spawn cmd.exe with the command
    // Using CREATE_NO_WINDOW | DETACHED_PROCESS to fully detach
    let child = Command::new("cmd.exe")
        .args(["/C", &shell_cmd])
        .current_dir(working_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .map_err(|e| format!("Failed to spawn cmd.exe: {e}"))?;

    let windows_pid = child.id();
    log::trace!("Native Windows process spawned with PID: {windows_pid} (placeholder, Claude runs as child)");

    // Return 0 as placeholder - caller relies on output file for status
    // We could return windows_pid, but the actual Claude process is a child of cmd.exe
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_escape() {
        assert_eq!(shell_escape("hello"), "'hello'");
        assert_eq!(shell_escape("hello world"), "'hello world'");
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
        assert_eq!(shell_escape(""), "''");
    }

    #[test]
    fn test_is_process_alive() {
        // Current process should be alive
        let pid = std::process::id();
        assert!(is_process_alive(pid));

        // Non-existent PID should not be alive
        assert!(!is_process_alive(999999));
    }
}
