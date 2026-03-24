use std::path::PathBuf;

use clap::{Parser, Subcommand};
use zed_claude_bridge::{lock, lsp, server};

#[derive(Parser)]
#[command(name = "zed-claude-bridge", version, about = "Bridge between Zed editor and Claude Code CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Workspace folder path (for server mode)
    workspace: Option<PathBuf>,

    /// Run as a language server on stdio (used by Zed extension)
    #[arg(long)]
    stdio: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Send editor selection to the bridge (reads ZED_* env vars)
    SendSelection,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Some(Commands::SendSelection) = cli.command {
        send_selection().await;
        return;
    }

    // Server mode
    tracing_subscriber::fmt::init();
    lock::install_panic_hook();

    let workspace = cli.workspace.expect("workspace path required");
    let resolved = workspace
        .canonicalize()
        .expect("workspace folder not found");

    let workspace_str = resolved.to_string_lossy().into_owned();
    let handle = server::start_server(vec![workspace_str.clone()]).await;
    lock::write_lock_file(handle.port, &[workspace_str.clone()], &handle.auth_token);

    let port = handle.port;

    if cli.stdio {
        eprintln!("zed-claude-bridge started (stdio mode)");
        eprintln!("  Port: {port}");
        lsp::run_stdio_lsp().await;
    } else {
        // SAFETY: This is set once during startup. No child processes have been spawned
        // yet that would read this variable. The tokio runtime threads do not access it.
        // This env var is only read by `claude` CLI processes started later in this terminal.
        unsafe {
            std::env::set_var("CLAUDE_CODE_SSE_PORT", port.to_string());
        }

        eprintln!("zed-claude-bridge started");
        eprintln!("  Port: {port}");
        eprintln!("  Lock: ~/.claude/ide/{port}.lock");
        eprintln!("  Workspace: {workspace_str}");
        eprintln!();
        eprintln!("Start Claude Code in this terminal to connect.");

        #[cfg(unix)]
        {
            let mut sigterm =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = sigterm.recv() => {}
            }
        }

        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c().await.ok();
        }
    }

    lock::cleanup(port);
}

async fn send_selection() {
    let text = std::env::var("ZED_SELECTED_TEXT").unwrap_or_default();
    let file = std::env::var("ZED_FILE").unwrap_or_default();
    let row: u32 = std::env::var("ZED_ROW")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let column: u32 = std::env::var("ZED_COLUMN")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let language = std::env::var("ZED_LANGUAGE").unwrap_or_default();

    // Find latest lock file
    let ide_dir = dirs::home_dir()
        .expect("no home dir")
        .join(".claude")
        .join("ide");

    let mut locks: Vec<_> = std::fs::read_dir(&ide_dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "lock"))
        .collect();

    locks.sort_by_key(|e| std::cmp::Reverse(e.metadata().ok().and_then(|m| m.modified().ok())));

    let lock_path = match locks.first() {
        Some(e) => e.path(),
        None => {
            eprintln!("No bridge lock file found");
            std::process::exit(1);
        }
    };

    let port: u16 = lock_path
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.parse().ok())
        .expect("invalid lock file name");

    let lock_data: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&lock_path).expect("can't read lock file"))
            .expect("invalid lock file JSON");

    let auth_token = lock_data["authToken"]
        .as_str()
        .expect("no authToken in lock file");

    let body = serde_json::json!({
        "text": text,
        "filePath": file,
        "row": row,
        "column": column,
        "language": language,
    });

    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("failed to create HTTP client");

    let resp = client
        .post(format!("http://127.0.0.1:{port}/api/selection"))
        .header("Content-Type", "application/json")
        .header("x-claude-code-ide-authorization", auth_token)
        .json(&body)
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {}
        Ok(r) => {
            eprintln!("Bridge returned {}", r.status());
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to send selection: {e}");
            std::process::exit(1);
        }
    }
}
