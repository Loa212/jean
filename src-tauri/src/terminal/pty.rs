use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::Read;
use std::sync::Mutex;
use std::thread;
use tauri::{AppHandle, Emitter};

use super::registry::{register_terminal, unregister_terminal};
use super::types::{
    TerminalOutputEvent, TerminalSession, TerminalStartedEvent, TerminalStoppedEvent,
};

/// Detect user's default shell (cross-platform)
fn get_user_shell() -> String {
    crate::platform::get_default_shell()
}

/// Spawn a terminal, optionally running a command
pub fn spawn_terminal(
    app: &AppHandle,
    terminal_id: String,
    worktree_path: String,
    cols: u16,
    rows: u16,
    command: Option<String>,
    use_wsl: bool,
) -> Result<(), String> {
    log::trace!("Spawning terminal {terminal_id} at {worktree_path} (use_wsl={use_wsl})");
    if let Some(ref cmd) = command {
        log::trace!("Running command: {cmd}");
    }

    let pty_system = native_pty_system();

    // Create PTY pair
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Failed to open PTY: {e}"))?;

    // Build command based on platform and mode
    #[cfg(windows)]
    let mut cmd = if use_wsl {
        // WSL mode: Use bash via WSL to handle Unix-style paths and commands
        use crate::platform::shell::{escape_for_bash, parse_wsl_path};

        let path_info = parse_wsl_path(&worktree_path);
        let escaped_path = escape_for_bash(&path_info.path);

        log::trace!(
            "Using WSL with path: {} (distribution: {:?})",
            path_info.path,
            path_info.distribution
        );

        let mut c = CommandBuilder::new("wsl");

        // Use specific distribution if available (for WSL UNC paths)
        if let Some(ref distro) = path_info.distribution {
            c.arg("-d");
            c.arg(distro);
        }

        c.arg("-e");
        c.arg("bash");
        c.arg("-c");

        if let Some(ref run_command) = command {
            // Run command through WSL bash
            c.arg(format!(
                "cd '{escaped_path}' && {run_command}; echo ''; echo '[Command finished. Press Ctrl+D to close]'; cat"
            ));
        } else {
            // Interactive shell in WSL
            c.arg(format!("cd '{escaped_path}' && exec bash"));
        }
        c
    } else {
        // Native mode: Use PowerShell
        log::trace!("Using PowerShell for terminal at: {worktree_path}");

        let mut c = CommandBuilder::new("powershell.exe");
        c.arg("-NoLogo");

        if let Some(ref run_command) = command {
            // Run command through PowerShell
            c.arg("-Command");
            c.arg(format!(
                "Set-Location -LiteralPath '{worktree_path}'; {run_command}; Write-Host ''; Write-Host '[Command finished. Press Enter to close]'; Read-Host"
            ));
        } else {
            // Interactive PowerShell session
            c.arg("-NoExit");
            c.arg("-Command");
            c.arg(format!("Set-Location -LiteralPath '{worktree_path}'"));
        }
        c
    };

    // On non-Windows, use the user's native shell
    #[cfg(not(windows))]
    let mut cmd = {
        let shell = get_user_shell();
        log::trace!("Using shell: {shell}");

        let mut c = if let Some(ref run_command) = command {
            // Run the command in shell, then keep shell open for inspection
            let mut c = CommandBuilder::new(&shell);
            c.arg("-c");
            // Run the command; if it exits, show message and wait for user
            // Note: Caller is responsible for properly quoting paths with spaces
            c.arg(format!(
                "{run_command}; echo ''; echo '[Command finished. Press Ctrl+D to close]'; cat"
            ));
            c
        } else {
            CommandBuilder::new(&shell)
        };
        c.cwd(&worktree_path);
        c
    };

    // Set environment variables
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("JEAN_WORKTREE_PATH", &worktree_path);

    // Spawn the shell
    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("Failed to spawn shell: {e}"))?;

    log::trace!("Spawned terminal process");

    // Get reader from master
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("Failed to clone reader: {e}"))?;

    // Get writer from master (must be taken once and stored)
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("Failed to take writer: {e}"))?;

    // Register the session
    let session = TerminalSession {
        terminal_id: terminal_id.clone(),
        master: pair.master,
        writer: Mutex::new(writer),
        child,
        cols,
        rows,
    };
    register_terminal(session);

    // Emit started event
    let started_event = TerminalStartedEvent {
        terminal_id: terminal_id.clone(),
        cols,
        rows,
    };
    if let Err(e) = app.emit("terminal:started", &started_event) {
        log::error!("Failed to emit terminal:started event: {e}");
    }

    // Spawn reader thread
    let app_clone = app.clone();
    let terminal_id_clone = terminal_id.clone();
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    // EOF - terminal closed
                    log::trace!("Terminal EOF for: {terminal_id_clone}");
                    break;
                }
                Ok(n) => {
                    // Convert bytes to string (lossy conversion for non-UTF8)
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    let event = TerminalOutputEvent {
                        terminal_id: terminal_id_clone.clone(),
                        data,
                    };
                    if let Err(e) = app_clone.emit("terminal:output", &event) {
                        log::error!("Failed to emit terminal:output event: {e}");
                    }
                }
                Err(e) => {
                    log::error!("Error reading from terminal: {e}");
                    break;
                }
            }
        }

        // Terminal has exited, get exit code and cleanup
        if let Some(mut session) = unregister_terminal(&terminal_id_clone) {
            let exit_code = session.child.wait().ok().and_then(|s| {
                if s.success() {
                    Some(0)
                } else {
                    // portable-pty ExitStatus doesn't expose code directly
                    None
                }
            });

            let stopped_event = TerminalStoppedEvent {
                terminal_id: terminal_id_clone,
                exit_code,
            };
            if let Err(e) = app_clone.emit("terminal:stopped", &stopped_event) {
                log::error!("Failed to emit terminal:stopped event: {e}");
            }
        }
    });

    Ok(())
}

/// Write data to a terminal
pub fn write_to_terminal(terminal_id: &str, data: &str) -> Result<(), String> {
    use std::io::Write;

    super::registry::with_terminal(terminal_id, |session| {
        let mut writer = session
            .writer
            .lock()
            .map_err(|e| format!("Failed to lock writer: {e}"))?;
        writer
            .write_all(data.as_bytes())
            .map_err(|e| format!("Failed to write: {e}"))?;
        writer.flush().map_err(|e| format!("Failed to flush: {e}"))
    })
    .ok_or_else(|| "Terminal not found".to_string())?
}

/// Resize a terminal
pub fn resize_terminal(terminal_id: &str, cols: u16, rows: u16) -> Result<(), String> {
    super::registry::with_terminal(terminal_id, |session| {
        session
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Failed to resize: {e}"))?;
        session.cols = cols;
        session.rows = rows;
        Ok(())
    })
    .ok_or_else(|| "Terminal not found".to_string())?
}

/// Kill a terminal
pub fn kill_terminal(app: &AppHandle, terminal_id: &str) -> Result<bool, String> {
    if let Some(mut session) = unregister_terminal(terminal_id) {
        // Kill the child process - try graceful termination first
        if let Some(pid) = session.child.process_id() {
            if let Err(e) = crate::platform::terminate_process(pid) {
                log::trace!("Graceful termination of pid={pid} failed: {e}");
            }
        }

        // Wait for the process to exit
        let _ = session.child.kill();

        // Emit stopped event
        let stopped_event = TerminalStoppedEvent {
            terminal_id: terminal_id.to_string(),
            exit_code: None,
        };
        if let Err(e) = app.emit("terminal:stopped", &stopped_event) {
            log::error!("Failed to emit terminal:stopped event: {e}");
        }

        Ok(true)
    } else {
        Ok(false)
    }
}

/// Kill all active terminals (used during app shutdown)
pub fn kill_all_terminals() -> usize {
    use super::registry::TERMINAL_SESSIONS;

    eprintln!("[TERMINAL CLEANUP] kill_all_terminals called");

    let mut sessions = TERMINAL_SESSIONS.lock().unwrap();
    let count = sessions.len();

    eprintln!("[TERMINAL CLEANUP] Found {count} active terminal(s)");

    for (terminal_id, mut session) in sessions.drain() {
        eprintln!("[TERMINAL CLEANUP] Killing terminal: {terminal_id}");

        if let Some(pid) = session.child.process_id() {
            eprintln!("[TERMINAL CLEANUP] Sending terminate signal to PID {pid}");
            if let Err(e) = crate::platform::terminate_process(pid) {
                eprintln!("[TERMINAL CLEANUP] Graceful termination failed: {e}");
            }
        }

        let _ = session.child.kill();
        eprintln!("[TERMINAL CLEANUP] Killed terminal: {terminal_id}");
    }

    eprintln!("[TERMINAL CLEANUP] Cleanup complete, killed {count} terminal(s)");

    count
}
