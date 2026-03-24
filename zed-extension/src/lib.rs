use zed_extension_api::{self as zed, Extension, LanguageServerId};

const GITHUB_REPO: &str = "seven332/zed-claude-bridge";
const BINARY_NAME: &str = "zed-claude-bridge";

struct ClaudeBridgeExtension {
    cached_binary_path: Option<String>,
}

impl ClaudeBridgeExtension {
    /// Get platform-specific asset name for GitHub release.
    fn asset_name() -> Result<String, String> {
        let (os, arch) = zed::current_platform();
        let os_str = match os {
            zed::Os::Mac => "apple-darwin",
            zed::Os::Linux => "unknown-linux-musl",
            zed::Os::Windows => return Err("Windows is not supported".into()),
        };
        let arch_str = match arch {
            zed::Architecture::Aarch64 => "aarch64",
            zed::Architecture::X8664 => "x86_64",
            zed::Architecture::X86 => return Err("x86 (32-bit) is not supported".into()),
        };
        Ok(format!("{BINARY_NAME}-{arch_str}-{os_str}.tar.gz"))
    }

    /// Ensure the binary is downloaded and return its path.
    fn ensure_binary(&mut self) -> Result<String, String> {
        // Return cached path if binary still exists
        if let Some(path) = &self.cached_binary_path {
            if std::fs::metadata(path).is_ok() {
                return Ok(path.clone());
            }
        }

        let asset_name = Self::asset_name()?;

        let release = zed::latest_github_release(
            GITHUB_REPO.into(),
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let asset = release
            .assets
            .iter()
            .find(|a| a.name == asset_name)
            .ok_or_else(|| {
                format!(
                    "No asset '{asset_name}' found in release {}",
                    release.version
                )
            })?;

        let version_dir = format!("download/{}", release.version);
        let binary_path = format!("{version_dir}/{BINARY_NAME}");

        // Already downloaded this version
        if std::fs::metadata(&binary_path).is_ok() {
            self.cached_binary_path = Some(binary_path.clone());
            return Ok(binary_path);
        }

        // Download and extract
        zed::download_file(
            &asset.download_url,
            &version_dir,
            zed::DownloadedFileType::GzipTar,
        )?;

        zed::make_file_executable(&binary_path)?;

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}

impl Extension for ClaudeBridgeExtension {
    fn new() -> Self {
        ClaudeBridgeExtension {
            cached_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command, String> {
        let binary = self.ensure_binary()?;

        Ok(zed::Command {
            command: binary,
            args: vec!["--stdio".into(), worktree.root_path()],
            env: Default::default(),
        })
    }
}

zed::register_extension!(ClaudeBridgeExtension);
