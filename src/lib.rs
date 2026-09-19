use std::fs;
use std::path::{Path, PathBuf};
use zed_extension_api::{self as zed, LanguageServerId, Result, serde_json, settings::LspSettings};

const SERVER_ID: &str = "janet-lsp-plus";
const SERVER_REPO: &str = "bondiano/janet-zed";
const SERVER_DIR_PREFIX: &str = "janet-lsp-plus-";
/// Left behind by versions that ran janet-lsp.
const JANET_LSP_DIR_PREFIX: &str = "janet-lsp-";

const JANET_SRC_DIR_PREFIX: &str = "janet-src-";
const BOOT_JANET: &str = "src/boot/boot.janet";

/// The debug adapter, `janet-lsp-plus dap`.
const ADAPTER: &str = "Janet";

struct JanetExtension {
    cached_server: Option<String>,
}

fn is_file(path: impl AsRef<Path>) -> bool {
    fs::metadata(path).is_ok_and(|meta| meta.is_file())
}

/// Reports installation progress when a language server is starting (not the debug adapter).
fn set_status(id: Option<&LanguageServerId>, status: &zed::LanguageServerInstallationStatus) {
    if let Some(id) = id {
        zed::set_language_server_installation_status(id, status);
    }
}

fn work_dir() -> Result<PathBuf> {
    std::env::current_dir().map_err(|err| format!("failed to resolve extension work dir: {err}"))
}

/// The release asset for this platform: its name, archive type and the binary inside.
fn server_asset() -> Result<(String, zed::DownloadedFileType, &'static str)> {
    let (os, arch) = zed::current_platform();
    let arch = match arch {
        zed::Architecture::Aarch64 => "aarch64",
        zed::Architecture::X8664 => "x86_64",
        zed::Architecture::X86 => return Err("32-bit x86 is not supported".to_string()),
    };
    Ok(match os {
        zed::Os::Mac => (
            format!("{SERVER_ID}-{arch}-apple-darwin.tar.gz"),
            zed::DownloadedFileType::GzipTar,
            SERVER_ID,
        ),
        zed::Os::Linux => (
            format!("{SERVER_ID}-{arch}-unknown-linux-gnu.tar.gz"),
            zed::DownloadedFileType::GzipTar,
            SERVER_ID,
        ),
        zed::Os::Windows => (
            format!("{SERVER_ID}-{arch}-pc-windows-msvc.zip"),
            zed::DownloadedFileType::Zip,
            "janet-lsp-plus.exe",
        ),
    })
}

impl JanetExtension {
    /// `janet-lsp-plus` from PATH (development), else this extension's release for this platform,
    /// downloaded into the extension work dir, else (offline) the one downloaded before.
    fn server_path(
        &mut self,
        id: Option<&LanguageServerId>,
        worktree: &zed::Worktree,
    ) -> Result<String> {
        if let Some(path) = worktree.which(SERVER_ID) {
            return Ok(path);
        }
        if let Some(path) = self.cached_server.as_ref().filter(|path| is_file(path)) {
            return Ok(path.clone());
        }

        let (asset_name, file_type, binary) = server_asset()?;
        let path = release_server(id, &asset_name, file_type, binary)
            .or_else(|err| installed_server(binary).ok_or(err));
        let status = match &path {
            Ok(_) => zed::LanguageServerInstallationStatus::None,
            Err(err) => zed::LanguageServerInstallationStatus::Failed(err.clone()),
        };
        set_status(id, &status);
        let path = path?;
        self.cached_server = Some(path.clone());
        Ok(path)
    }
}

/// The server binary released together with this extension, downloaded unless it already is.
/// Pinned to the extension's own tag: a newer release may rename its assets or change the
/// protocol, and an older extension must not pick it up.
fn release_server(
    id: Option<&LanguageServerId>,
    asset_name: &str,
    file_type: zed::DownloadedFileType,
    binary: &str,
) -> Result<String> {
    let tag = concat!("v", env!("CARGO_PKG_VERSION"));
    let dir = format!("{SERVER_DIR_PREFIX}{tag}");
    let path = format!("{dir}/{binary}");
    if is_file(&path) {
        return Ok(path);
    }
    set_status(
        id,
        &zed::LanguageServerInstallationStatus::CheckingForUpdate,
    );
    let release = zed::github_release_by_tag_name(SERVER_REPO, tag)?;
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .ok_or_else(|| format!("{SERVER_REPO} {tag} has no {asset_name}"))?;
    set_status(id, &zed::LanguageServerInstallationStatus::Downloading);
    zed::download_file(&asset.download_url, &dir, file_type)?;
    zed::make_file_executable(&path)?;
    remove_stale_versions(SERVER_DIR_PREFIX, &dir);
    remove_stale_versions(JANET_LSP_DIR_PREFIX, "");
    Ok(path)
}

/// The newest server binary an earlier start downloaded into the work dir.
fn installed_server(binary: &str) -> Option<String> {
    fs::read_dir(".")
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let version: Vec<u64> = name
                .strip_prefix(SERVER_DIR_PREFIX)?
                .split(|c: char| !c.is_ascii_digit())
                .filter_map(|part| part.parse().ok())
                .collect();
            let path = format!("{name}/{binary}");
            is_file(&path).then_some((version, path))
        })
        .max()
        .map(|(_, path)| path)
}

fn remove_stale_versions(prefix: &str, current_dir: &str) {
    let Ok(entries) = fs::read_dir(".") else {
        return;
    };
    entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with(prefix) && name != current_dir
        })
        .for_each(|entry| {
            fs::remove_dir_all(entry.path()).ok();
        });
}

/// The Janet repo root inside `dir`: either `dir` itself or its single top-level archive folder.
fn find_janet_root(dir: &str) -> Option<PathBuf> {
    let dir = Path::new(dir);
    if is_file(dir.join(BOOT_JANET)) {
        return Some(dir.to_path_buf());
    }
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| is_file(path.join(BOOT_JANET)))
}

/// Janet sources matching the installed `janet`, downloaded into the extension work dir.
fn downloaded_janet_source(id: &LanguageServerId, janet: &str) -> Result<PathBuf> {
    let output = zed::process::Command::new(janet)
        .args(["-e", "(prin janet/version)"])
        .output()?;
    if output.status != Some(0) {
        return Err(format!(
            "`janet -e` failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let dir = format!("{JANET_SRC_DIR_PREFIX}{version}");

    let root = if let Some(root) = find_janet_root(&dir) {
        root
    } else {
        zed::set_language_server_installation_status(
            id,
            &zed::LanguageServerInstallationStatus::Downloading,
        );
        zed::download_file(
            &format!("https://github.com/janet-lang/janet/archive/refs/tags/v{version}.tar.gz"),
            &dir,
            zed::DownloadedFileType::GzipTar,
        )?;
        remove_stale_versions(JANET_SRC_DIR_PREFIX, &dir);
        zed::set_language_server_installation_status(
            id,
            &zed::LanguageServerInstallationStatus::None,
        );
        find_janet_root(&dir)
            .ok_or_else(|| format!("Janet {version} archive has no {BOOT_JANET}"))?
    };
    Ok(work_dir()?.join(root))
}

/// `lsp.janet-lsp-plus.settings.<key>` in Zed settings.
fn configured(worktree: &zed::Worktree, key: &str) -> Option<serde_json::Value> {
    LspSettings::for_worktree(SERVER_ID, worktree)
        .ok()?
        .settings?
        .get(key)
        .cloned()
}

impl zed::Extension for JanetExtension {
    fn new() -> Self {
        Self {
            cached_server: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        // `lsp.janet-lsp-plus.binary.env` goes on top, e.g. `JANET_LSP_LOG=debug`.
        let settings_env = LspSettings::for_worktree(SERVER_ID, worktree)
            .ok()
            .and_then(|settings| settings.binary)
            .and_then(|binary| binary.env)
            .unwrap_or_default();
        Ok(zed::Command {
            command: self.server_path(Some(language_server_id), worktree)?,
            args: vec![],
            env: worktree
                .shell_env()
                .into_iter()
                .chain(settings_env)
                .collect(),
        })
    }

    fn language_server_initialization_options(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<serde_json::Value>> {
        // Both options are optional to the server, which runs what needs no Janet without them.
        let Some(janet) = worktree.which("janet") else {
            eprintln!("`janet` is not in PATH: checks, formatting and core docs are off");
            return Ok(None);
        };
        let janet_source = configured(worktree, "janet_source")
            .and_then(|value| value.as_str().map(str::to_string))
            .or_else(|| {
                downloaded_janet_source(language_server_id, &janet)
                    .inspect_err(|err| {
                        eprintln!("no Janet source, stdlib go-to-definition is off: {err}");
                        zed::set_language_server_installation_status(
                            language_server_id,
                            &zed::LanguageServerInstallationStatus::None,
                        );
                    })
                    .ok()
                    .map(|path| path.to_string_lossy().into_owned())
            });
        Ok(Some(serde_json::json!({
            "janetPath": janet,
            "janetSource": janet_source,
            // `{"diagnostics": "off" | "hint" | "warning"}`; the server changes it later on
            // `didChangeConfiguration` too.
            "types": configured(worktree, "types"),
            // `false` stops compiling open files, which runs the project's code.
            "compile": configured(worktree, "compile"),
            // The kernelspec of Zed's REPL: on unless `false`, which removes it.
            "kernel": configured(worktree, "kernel").unwrap_or_else(|| true.into()),
        })))
    }

    /// Sent with `didChangeConfiguration`. `types` is always an object, `{}` once the user removes
    /// it: the server keeps its settings when the key is missing, and resets them on `{}`.
    fn language_server_workspace_configuration(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<serde_json::Value>> {
        Ok(Some(serde_json::json!({
            "types": configured(worktree, "types").unwrap_or_else(|| serde_json::json!({})),
        })))
    }

    fn get_dap_binary(
        &mut self,
        adapter_name: String,
        config: zed::DebugTaskDefinition,
        user_provided_debug_adapter_path: Option<String>,
        worktree: &zed::Worktree,
    ) -> Result<zed::DebugAdapterBinary> {
        let mut configuration: serde_json::Value = serde_json::from_str(&config.config)
            .map_err(|err| format!("invalid {ADAPTER} debug config: {err}"))?;
        let request = self.dap_request_kind(adapter_name, configuration.clone())?;
        if let Some(options) = configuration.as_object_mut() {
            options
                .entry("cwd")
                .or_insert_with(|| worktree.root_path().into());
            if let Some(janet) = worktree.which("janet") {
                options.entry("janet").or_insert_with(|| janet.into());
            }
        }
        let command = match user_provided_debug_adapter_path {
            Some(path) => path,
            None => self.server_path(None, worktree)?,
        };
        Ok(zed::DebugAdapterBinary {
            command: Some(command),
            arguments: vec!["dap".to_string()],
            envs: worktree.shell_env(),
            cwd: Some(worktree.root_path()),
            connection: None,
            request_args: zed::StartDebuggingRequestArguments {
                configuration: configuration.to_string(),
                request,
            },
        })
    }

    fn dap_request_kind(
        &mut self,
        _adapter_name: String,
        config: serde_json::Value,
    ) -> Result<zed::StartDebuggingRequestArgumentsRequest> {
        match config.get("request").and_then(serde_json::Value::as_str) {
            Some("launch") => Ok(zed::StartDebuggingRequestArgumentsRequest::Launch),
            Some("attach") => Ok(zed::StartDebuggingRequestArgumentsRequest::Attach),
            other => Err(format!(
                "`request` must be \"launch\" or \"attach\", not {other:?}"
            )),
        }
    }

    /// The "new session" modal: a program to launch, or the REPL kernel to attach to.
    fn dap_config_to_scenario(&mut self, config: zed::DebugConfig) -> Result<zed::DebugScenario> {
        let configuration = match config.request {
            zed::DebugRequest::Launch(launch) => {
                let env: serde_json::Map<String, serde_json::Value> = launch
                    .envs
                    .into_iter()
                    .map(|(name, value)| (name, value.into()))
                    .collect();
                let mut configuration = serde_json::json!({
                    "request": "launch",
                    "program": launch.program,
                    "args": launch.args,
                    "env": env,
                    "stopOnEntry": config.stop_on_entry.unwrap_or(false),
                });
                if let Some(cwd) = launch.cwd {
                    configuration["cwd"] = cwd.into();
                }
                configuration
            }
            zed::DebugRequest::Attach(_) => serde_json::json!({"request": "attach"}),
        };
        Ok(zed::DebugScenario {
            label: config.label,
            adapter: config.adapter,
            build: None,
            config: configuration.to_string(),
            tcp_connection: None,
        })
    }

    /// The `Janet: run` task (`janet <file>`) as a launch scenario.
    fn dap_locator_create_scenario(
        &mut self,
        _locator_name: String,
        build_task: zed::TaskTemplate,
        resolved_label: String,
        _debug_adapter_name: String,
    ) -> Option<zed::DebugScenario> {
        let [program] = build_task.args.as_slice() else {
            return None;
        };
        if build_task.command != "janet"
            || !(program == "$ZED_FILE"
                || Path::new(program)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("janet")))
        {
            return None;
        }
        let mut configuration = serde_json::json!({"request": "launch", "program": program});
        if let Some(cwd) = build_task.cwd {
            configuration["cwd"] = cwd.into();
        }
        Some(zed::DebugScenario {
            label: resolved_label,
            adapter: ADAPTER.to_string(),
            build: None,
            config: configuration.to_string(),
            tcp_connection: None,
        })
    }
}

zed::register_extension!(JanetExtension);
