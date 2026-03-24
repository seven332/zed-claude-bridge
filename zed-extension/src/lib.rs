use zed_extension_api::{self as zed, Extension, LanguageServerId};

struct ClaudeBridgeExtension;

impl Extension for ClaudeBridgeExtension {
    fn new() -> Self {
        ClaudeBridgeExtension
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command, String> {
        let binary = worktree.which("zed-claude-bridge").ok_or_else(|| {
            "zed-claude-bridge not found in PATH. \
             Install with: cargo install --path . (from the zed-claude-bridge repo)"
                .to_string()
        })?;

        Ok(zed::Command {
            command: binary,
            args: vec!["--stdio".into(), worktree.root_path()],
            env: Default::default(),
        })
    }
}

zed::register_extension!(ClaudeBridgeExtension);
