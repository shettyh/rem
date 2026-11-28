use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};

/// Wait for the exiting macOS app process to terminate, then launch the updated
/// `.app` bundle in the foreground via LaunchServices.
///
/// Tauri 2.12's built-in `tauri::process::restart` invokes `/usr/bin/open -n -g`
/// while the old instance is still running. Passing `-g` (`--background`) causes
/// LaunchServices to set `dontMakeFrontmost=1`, leaving the restarted app window
/// occluded behind other apps, while `-n` racing the exiting PID creates a
/// duplicate unpinned Dock tile.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const MACOS_RELAUNCH_SCRIPT: &str = r#"pid="$1"
bundle="$2"
shift 2
i=0
while kill -0 "$pid" 2>/dev/null && [ "$i" -lt 200 ]; do
  sleep 0.05
  i=$((i + 1))
done
if kill -0 "$pid" 2>/dev/null; then
  if [ "$#" -gt 0 ]; then
    exec /usr/bin/open -n "$bundle" --args "$@"
  else
    exec /usr/bin/open -n "$bundle"
  fi
else
  sleep 0.1
  if [ "$#" -gt 0 ]; then
    exec /usr/bin/open "$bundle" --args "$@"
  else
    exec /usr/bin/open "$bundle"
  fi
fi"#;

/// Resolve `/path/to/App.app` from `/path/to/App.app/Contents/MacOS/<binary>`.
/// Returns `None` for non-bundle binaries (such as `tauri dev`).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn macos_app_bundle_root(binary_path: &Path) -> Option<PathBuf> {
    let macos_dir = binary_path.parent()?;
    if macos_dir.components().next_back() != Some(Component::Normal(OsStr::new("MacOS"))) {
        return None;
    }

    let contents_dir = macos_dir.parent()?;
    if contents_dir.components().next_back() != Some(Component::Normal(OsStr::new("Contents"))) {
        return None;
    }

    let bundle_root = contents_dir.parent()?;
    if bundle_root.extension() != Some(OsStr::new("app")) {
        return None;
    }
    if bundle_root.file_stem().map(OsStr::is_empty).unwrap_or(true) {
        return None;
    }

    Some(bundle_root.to_path_buf())
}

/// Extract command-line arguments to forward across relaunch, skipping argv[0]
/// and any macOS LaunchServices process serial number flag (`-psn_*`).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn forwardable_relaunch_args(args_os: &[OsString]) -> Vec<OsString> {
    args_os
        .iter()
        .skip(1)
        .filter(|arg| !arg.to_string_lossy().starts_with("-psn_"))
        .cloned()
        .collect()
}

/// Build the argument vector passed to `/bin/sh` for the detached macOS
/// relaunch helper. Paths and arguments are passed as positional parameters so
/// spaces or shell metacharacters are never interpreted.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn build_macos_relaunch_args(
    pid: u32,
    bundle_root: &Path,
    extra_args: &[OsString],
) -> Vec<OsString> {
    let mut args = Vec::with_capacity(5 + extra_args.len());
    args.push(OsString::from("-c"));
    args.push(OsString::from(MACOS_RELAUNCH_SCRIPT));
    args.push(OsString::from("rem-relaunch"));
    args.push(OsString::from(pid.to_string()));
    args.push(bundle_root.as_os_str().to_os_string());
    args.extend(extra_args.iter().cloned());
    args
}

#[cfg(target_os = "macos")]
fn try_relaunch_macos_bundle<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use tauri::Manager;

    let env = app.env();
    // Use Tauri's startup-cached binary path rather than `std::env::current_exe`,
    // because `tauri-plugin-updater` atomically swaps the `.app` bundle on disk
    // while the previous instance is still running.
    let Ok(current_bin) = tauri::process::current_binary(&env) else {
        return false;
    };
    let Some(bundle_root) = macos_app_bundle_root(&current_bin) else {
        return false;
    };
    if !bundle_root.is_dir() {
        return false;
    }

    let extra_args = forwardable_relaunch_args(&env.args_os);
    let sh_args = build_macos_relaunch_args(std::process::id(), &bundle_root, &extra_args);

    match Command::new("/bin/sh")
        .args(&sh_args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
    {
        Ok(_) => true,
        Err(err) => {
            log::error!("failed to spawn macOS relaunch helper: {err}");
            false
        }
    }
}

#[tauri::command]
pub fn app_relaunch<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    #[cfg(target_os = "macos")]
    if try_relaunch_macos_bundle(&app) {
        app.exit(0);
        return;
    }

    app.request_restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_macos_app_bundle_root() {
        assert_eq!(
            macos_app_bundle_root(Path::new("/Applications/rem.app/Contents/MacOS/app")),
            Some(PathBuf::from("/Applications/rem.app"))
        );
        assert_eq!(
            macos_app_bundle_root(Path::new(
                "/Users/test/My Apps/rem beta.app/Contents/MacOS/app"
            )),
            Some(PathBuf::from("/Users/test/My Apps/rem beta.app"))
        );
    }

    #[test]
    fn ignores_non_bundle_paths() {
        assert_eq!(
            macos_app_bundle_root(Path::new("/Users/test/rem/src-tauri/target/debug/app")),
            None
        );
        assert_eq!(
            macos_app_bundle_root(Path::new("/Applications/rem/Contents/MacOS/app")),
            None
        );
        assert_eq!(
            macos_app_bundle_root(Path::new("/Applications/.app/Contents/MacOS/app")),
            None
        );
    }

    #[test]
    fn filters_argv0_and_launchservices_psn_args() {
        let raw = vec![
            OsString::from("/Applications/rem.app/Contents/MacOS/app"),
            OsString::from("-psn_0_123456"),
            OsString::from("--profile"),
            OsString::from("work"),
        ];
        assert_eq!(
            forwardable_relaunch_args(&raw),
            vec![OsString::from("--profile"), OsString::from("work")]
        );
    }

    #[test]
    fn builds_foreground_relaunch_script_without_background_flag() {
        let args = build_macos_relaunch_args(
            4242,
            Path::new("/Applications/rem.app"),
            &[OsString::from("--flag")],
        );
        assert_eq!(args[0], OsStr::new("-c"));
        assert_eq!(args[2], OsStr::new("rem-relaunch"));
        assert_eq!(args[3], OsStr::new("4242"));
        assert_eq!(args[4], OsStr::new("/Applications/rem.app"));
        assert_eq!(args[5], OsStr::new("--flag"));

        let open_lines: Vec<&str> = MACOS_RELAUNCH_SCRIPT
            .lines()
            .filter(|line| line.contains("/usr/bin/open"))
            .collect();
        assert!(!open_lines.is_empty());
        for line in open_lines {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            assert!(
                !tokens.contains(&"-g") && !tokens.contains(&"--background"),
                "open invocation must not launch in background: {line}"
            );
        }
        assert!(MACOS_RELAUNCH_SCRIPT.contains("/usr/bin/open \"$bundle\""));
    }

    #[cfg(unix)]
    #[test]
    fn macos_relaunch_script_has_valid_posix_sh_syntax() {
        let output = std::process::Command::new("/bin/sh")
            .arg("-n")
            .arg("-c")
            .arg(MACOS_RELAUNCH_SCRIPT)
            .output()
            .expect("failed to invoke /bin/sh -n");
        assert!(
            output.status.success(),
            "relaunch script failed sh -n: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
