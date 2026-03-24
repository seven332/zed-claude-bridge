use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};

fn lock_dir() -> PathBuf {
    dirs::home_dir()
        .expect("could not find home directory")
        .join(".claude")
        .join("ide")
}

fn lock_path(port: u16) -> PathBuf {
    lock_dir().join(format!("{port}.lock"))
}

/// Check if another bridge instance is already running for this workspace.
/// Returns true if a live lock file exists with a matching workspace folder.
pub fn already_running(workspace: &str) -> bool {
    let dir = lock_dir();
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return false,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("lock") {
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let data: serde_json::Value = match serde_json::from_str(&content) {
            Ok(d) => d,
            Err(_) => continue,
        };
        // Check if workspace matches
        let folders = data["workspaceFolders"].as_array();
        let has_workspace = folders
            .map(|f| f.iter().any(|v| v.as_str() == Some(workspace)))
            .unwrap_or(false);
        if !has_workspace {
            continue;
        }
        // Check if process is alive
        if let Some(pid) = data["pid"].as_u64() {
            let pid = pid as i32;
            #[cfg(unix)]
            {
                // kill(pid, 0) checks if process exists without sending a signal
                if unsafe { libc::kill(pid, 0) } == 0 {
                    return true;
                }
            }
            #[cfg(not(unix))]
            {
                let _ = pid;
            }
        }
    }
    false
}

/// Symlink the current binary to ~/.claude/bin/zed-claude-bridge so tasks can find it.
pub fn install_to_claude_bin() {
    let current_exe = match std::env::current_exe().and_then(|p| p.canonicalize()) {
        Ok(p) => p,
        Err(_) => return,
    };
    let bin_dir = dirs::home_dir()
        .expect("could not find home directory")
        .join(".claude")
        .join("bin");
    let _ = fs::create_dir_all(&bin_dir);
    let link_path = bin_dir.join("zed-claude-bridge");
    // Remove old symlink/file if it exists
    let _ = fs::remove_file(&link_path);
    #[cfg(unix)]
    {
        let _ = std::os::unix::fs::symlink(&current_exe, &link_path);
    }
    #[cfg(not(unix))]
    {
        let _ = fs::copy(&current_exe, &link_path);
    }
}

pub fn write_lock_file(port: u16, workspace_folders: &[String], auth_token: &str) {
    let dir = lock_dir();
    fs::create_dir_all(&dir).expect("failed to create lock dir");
    #[cfg(unix)]
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))
        .expect("failed to set lock dir permissions");

    let lock_data = serde_json::json!({
        "pid": std::process::id(),
        "workspaceFolders": workspace_folders,
        "ideName": "Zed",
        "transport": "ws",
        "runningInWindows": false,
        "authToken": auth_token,
    });

    let path = lock_path(port);
    fs::write(&path, lock_data.to_string()).expect("failed to write lock file");
    #[cfg(unix)]
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
        .expect("failed to set lock file permissions");

    // Store port so panic hook can clean up
    CLEANUP_PORT.store(port, Ordering::SeqCst);
}

pub fn remove_lock_file(port: u16) {
    let _ = fs::remove_file(lock_path(port));
}

static CLEANED_UP: AtomicBool = AtomicBool::new(false);
static CLEANUP_PORT: AtomicU16 = AtomicU16::new(0);

pub fn cleanup(port: u16) {
    if CLEANED_UP
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        remove_lock_file(port);
    }
}

/// Install a panic hook that removes the lock file on panic.
pub fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let port = CLEANUP_PORT.load(Ordering::SeqCst);
        if port != 0 {
            cleanup(port);
        }
        prev(info);
    }));
}
