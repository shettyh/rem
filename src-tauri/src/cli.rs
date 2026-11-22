use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CliStatus {
    pub supported: bool,
    pub bundled_path: Option<String>,
    pub installed_path: Option<String>,
    pub managed_by_homebrew: bool,
    pub linked_to_bundled: bool,
    pub on_path: bool,
}

fn cli_binary_name() -> &'static str {
    if cfg!(windows) {
        "rem.exe"
    } else {
        "rem"
    }
}

fn bundled_cli_path_from_exe(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    let candidate = dir.join(cli_binary_name());
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

fn candidate_install_paths(
    home: Option<&Path>,
    path_env: Option<&std::ffi::OsStr>,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let bin = cli_binary_name();

    if let Some(paths) = path_env {
        for dir in std::env::split_paths(paths) {
            let p = dir.join(bin);
            if !candidates.contains(&p) {
                candidates.push(p);
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        for dir in ["/opt/homebrew/bin", "/usr/local/bin"] {
            let p = PathBuf::from(dir).join(bin);
            if !candidates.contains(&p) {
                candidates.push(p);
            }
        }
    }

    if let Some(home_dir) = home {
        let local_bin = home_dir.join(".local").join("bin").join(bin);
        if !candidates.contains(&local_bin) {
            candidates.push(local_bin);
        }
    }

    candidates
}

fn dir_is_on_path(dir: &Path, path_env: Option<&std::ffi::OsStr>) -> bool {
    let Some(paths) = path_env else {
        return false;
    };
    let canonical_dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    std::env::split_paths(paths).any(|entry| {
        if entry == dir {
            return true;
        }
        std::fs::canonicalize(&entry)
            .map(|c| c == canonical_dir)
            .unwrap_or(false)
    })
}

fn is_homebrew_path(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.starts_with("/opt/homebrew/") || text.starts_with("/usr/local/Cellar/")
}

fn inspect_cli_status(
    bundled_path: Option<PathBuf>,
    home: Option<&Path>,
    path_env: Option<&std::ffi::OsStr>,
) -> CliStatus {
    let supported = cfg!(unix) && bundled_path.is_some();
    let canonical_bundled = bundled_path
        .as_deref()
        .and_then(|p| std::fs::canonicalize(p).ok());

    let mut installed_path: Option<PathBuf> = None;
    for candidate in candidate_install_paths(home, path_env) {
        if candidate.exists() {
            installed_path = Some(candidate);
            break;
        }
    }

    let linked_to_bundled = match (&installed_path, &canonical_bundled) {
        (Some(installed), Some(bundled)) => std::fs::canonicalize(installed)
            .map(|resolved| &resolved == bundled)
            .unwrap_or(false),
        _ => false,
    };

    let managed_by_homebrew = installed_path
        .as_deref()
        .map(is_homebrew_path)
        .unwrap_or(false);

    let on_path = installed_path
        .as_deref()
        .and_then(Path::parent)
        .map(|parent| dir_is_on_path(parent, path_env))
        .unwrap_or(false);

    CliStatus {
        supported,
        bundled_path: bundled_path.map(|p| p.display().to_string()),
        installed_path: installed_path.map(|p| p.display().to_string()),
        managed_by_homebrew,
        linked_to_bundled,
        on_path,
    }
}

#[cfg(unix)]
fn install_symlink(bundled_path: &Path, dest_dir: &Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::symlink;

    std::fs::create_dir_all(dest_dir)
        .map_err(|err| format!("Could not create {}: {err}", dest_dir.display()))?;

    let target = dest_dir.join(cli_binary_name());
    let tmp_link = dest_dir.join(format!(".rem-cli-link-{}.tmp", std::process::id()));
    let _ = std::fs::remove_file(&tmp_link);

    symlink(bundled_path, &tmp_link)
        .map_err(|err| format!("Could not create CLI symlink: {err}"))?;
    std::fs::rename(&tmp_link, &target).map_err(|err| {
        let _ = std::fs::remove_file(&tmp_link);
        format!("Could not install CLI at {}: {err}", target.display())
    })?;

    Ok(target)
}

#[tauri::command]
pub fn cli_status() -> CliStatus {
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|exe| bundled_cli_path_from_exe(&exe));
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let path_env = std::env::var_os("PATH");
    inspect_cli_status(bundled, home.as_deref(), path_env.as_deref())
}

#[tauri::command]
pub fn cli_install() -> Result<CliStatus, String> {
    #[cfg(not(unix))]
    {
        return Err("Automatic CLI installation is currently supported on macOS and Linux.".into());
    }

    #[cfg(unix)]
    {
        let exe = std::env::current_exe()
            .map_err(|err| format!("Could not locate the rem app executable: {err}"))?;
        let bundled = bundled_cli_path_from_exe(&exe).ok_or_else(|| {
            "The bundled rem CLI binary is not present in this development build.".to_string()
        })?;
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "HOME environment variable is not set.".to_string())?;
        let dest_dir = home.join(".local").join("bin");
        install_symlink(&bundled, &dest_dir)?;

        let path_env = std::env::var_os("PATH");
        Ok(inspect_cli_status(
            Some(bundled),
            Some(home.as_path()),
            path_env.as_deref(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn detects_bundled_cli_next_to_executable() {
        let dir = std::env::temp_dir().join(format!("rem-cli-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let app_exe = dir.join("app");
        let cli_bin = dir.join(cli_binary_name());
        std::fs::write(&app_exe, b"gui").unwrap();
        assert_eq!(bundled_cli_path_from_exe(&app_exe), None);

        std::fs::write(&cli_bin, b"cli").unwrap();
        assert_eq!(bundled_cli_path_from_exe(&app_exe), Some(cli_bin));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn installs_and_recognizes_symlink_to_bundled_cli() {
        let root = std::env::temp_dir().join(format!("rem-cli-link-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bundle_dir = root.join("rem.app/Contents/MacOS");
        let home_dir = root.join("home");
        std::fs::create_dir_all(&bundle_dir).unwrap();
        std::fs::create_dir_all(&home_dir).unwrap();

        let bundled = bundle_dir.join("rem");
        std::fs::write(&bundled, b"#!/bin/sh\necho rem\n").unwrap();

        let dest_dir = home_dir.join(".local/bin");
        let installed = install_symlink(&bundled, &dest_dir).unwrap();
        assert_eq!(installed, dest_dir.join("rem"));

        let path_env = OsString::from(&dest_dir);
        let status = inspect_cli_status(
            Some(bundled.clone()),
            Some(home_dir.as_path()),
            Some(path_env.as_os_str()),
        );
        assert!(status.supported);
        assert!(status.linked_to_bundled);
        assert!(status.on_path);
        assert!(!status.managed_by_homebrew);
        assert_eq!(status.installed_path, Some(installed.display().to_string()));

        // Re-installing over an existing symlink succeeds cleanly.
        install_symlink(&bundled, &dest_dir).unwrap();

        let _ = std::fs::remove_dir_all(&root);
    }
}
