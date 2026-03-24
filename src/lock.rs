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
