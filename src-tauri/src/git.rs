use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct CommitPushResult {
    pushed: bool,
    rejected: bool,
}

/// Run git in `dir`, returning (stdout, stderr, success). Maps a missing git
/// binary to a recognizable error string.
fn run_git(args: &[&str], dir: &Path) -> Result<(String, String, bool), String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        // Never block on an interactive credential/passphrase prompt. If the
        // remote is inaccessible, git must fail fast so the error surfaces in
        // the UI instead of hanging on the launching terminal's stdin.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "git-not-installed".to_string()
            } else {
                format!("failed to run git: {e}")
            }
        })?;
    Ok((
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.success(),
    ))
}

fn ok_or_stderr(res: (String, String, bool)) -> Result<String, String> {
    let (stdout, stderr, success) = res;
    if success {
        Ok(stdout)
    } else {
        Err(stderr)
    }
}

#[derive(Serialize, Deserialize)]
pub struct AssetFile {
    name: String,
    /// base64-encoded file bytes (transport only; on disk the bytes are raw).
    data: String,
}

/// Git working copy rooted once from trusted application state.
pub(crate) struct GitRepo {
    root: PathBuf,
}

impl GitRepo {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    async fn is_cloned(&self) -> Result<bool, String> {
        reject_symlinked_root(&self.root)?;
        Ok(self.root.join(".git").exists())
    }

    async fn clone_remote(&self, remote_url: String) -> Result<(), String> {
        reject_symlinked_root(&self.root)?;
        let parent = self.root.parent().ok_or("invalid repo dir")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let name = self
            .root
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("invalid repo dir")?;
        ok_or_stderr(run_git(&["clone", "--", &remote_url, name], parent)?)?;
        Ok(())
    }

    async fn set_remote_url(&self, remote_url: String) -> Result<(), String> {
        reject_symlinked_root(&self.root)?;
        ok_or_stderr(run_git(
            &["remote", "set-url", "--", "origin", &remote_url],
            &self.root,
        )?)?;
        Ok(())
    }

    async fn fetch_reset(&self) -> Result<bool, String> {
        reject_symlinked_root(&self.root)?;
        ok_or_stderr(run_git(&["fetch", "origin"], &self.root)?)?;
        let (_, _, has_main) = run_git(&["rev-parse", "--verify", "origin/main"], &self.root)?;
        if !has_main {
            return Ok(false);
        }
        ok_or_stderr(run_git(&["reset", "--hard", "origin/main"], &self.root)?)?;
        Ok(true)
    }

    async fn read_files(&self) -> Result<HashMap<String, String>, String> {
        reject_symlinked_root(&self.root)?;
        let mut out = HashMap::new();
        collect_files(&self.root, &mut out)?;
        Ok(out)
    }

    async fn write_files(&self, files: HashMap<String, String>) -> Result<(), String> {
        reject_symlinked_root(&self.root)?;
        for path in files.keys() {
            validate_snapshot_path(path)?;
        }
        validate_managed_type(&self.root, "decks", true)?;
        validate_managed_type(&self.root, "rem.json", false)?;
        validate_managed_type(&self.root, "tombstones.json", false)?;
        remove_dir_if_exists(self.root.join("decks"))?;
        remove_file_if_exists(self.root.join("tombstones.json"))?;
        remove_file_if_exists(self.root.join("rem.json"))?;
        for (rel, content) in files {
            let full = self.root.join(&rel);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(&full, content).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    async fn commit_push(&self, message: String) -> Result<CommitPushResult, String> {
        reject_symlinked_root(&self.root)?;
        ok_or_stderr(run_git(&["add", "-A"], &self.root)?)?;
        let (_, _, has_changes) = run_git(&["diff", "--cached", "--quiet"], &self.root)?;
        if !has_changes {
            ok_or_stderr(run_git(
                &[
                    "-c",
                    "user.name=rem",
                    "-c",
                    "user.email=rem@localhost",
                    "commit",
                    "-m",
                    &message,
                ],
                &self.root,
            )?)?;
        }
        let (_, stderr, success) = run_git(&["push", "origin", "HEAD:main"], &self.root)?;
        if success {
            return Ok(CommitPushResult {
                pushed: true,
                rejected: false,
            });
        }
        let lower = stderr.to_lowercase();
        if lower.contains("rejected")
            || lower.contains("non-fast-forward")
            || lower.contains("fetch first")
        {
            return Ok(CommitPushResult {
                pushed: false,
                rejected: true,
            });
        }
        Err(stderr)
    }

    async fn read_assets(&self) -> Result<Vec<AssetFile>, String> {
        reject_symlinked_root(&self.root)?;
        validate_managed_type(&self.root, "assets", true)?;
        let assets_dir = self.root.join("assets");
        let mut out = Vec::new();
        if !assets_dir.exists() {
            return Ok(out);
        }
        for entry in fs::read_dir(&assets_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if metadata.file_type().is_symlink() {
                return Err(format!("symlink not allowed: assets/{name}"));
            }
            if !metadata.is_file() {
                continue;
            }
            validate_asset_name(&name)?;
            let bytes = fs::read(&path).map_err(|e| e.to_string())?;
            out.push(AssetFile {
                name,
                data: STANDARD.encode(&bytes),
            });
        }
        Ok(out)
    }

    async fn write_assets(&self, assets: Vec<AssetFile>) -> Result<(), String> {
        reject_symlinked_root(&self.root)?;
        let decoded = assets
            .into_iter()
            .map(|asset| {
                validate_asset_name(&asset.name)?;
                let bytes = STANDARD.decode(&asset.data).map_err(|e| e.to_string())?;
                Ok((asset.name, bytes))
            })
            .collect::<Result<Vec<_>, String>>()?;
        validate_managed_type(&self.root, "assets", true)?;
        let assets_dir = self.root.join("assets");
        remove_dir_if_exists(&assets_dir)?;
        if !decoded.is_empty() {
            fs::create_dir_all(&assets_dir).map_err(|e| e.to_string())?;
        }
        for (name, bytes) in decoded {
            fs::write(assets_dir.join(name), bytes).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

fn reject_symlinked_root(root: &Path) -> Result<(), String> {
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err("repository root must not be a symlink".into());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    }
    match fs::symlink_metadata(root.join(".git")) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err("repository .git must not be a symlink".into())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn validate_managed_type(root: &Path, relative: &str, directory: bool) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(root.join(relative)) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if metadata.file_type().is_symlink() {
        return Err(format!("symlink not allowed: {relative}"));
    }
    if directory && !metadata.is_dir() {
        return Err(format!("snapshot path is not a directory: {relative}"));
    }
    if !directory && !metadata.is_file() {
        return Err(format!("snapshot path is not a file: {relative}"));
    }
    Ok(())
}

fn remove_dir_if_exists(path: impl AsRef<Path>) -> Result<(), String> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn remove_file_if_exists(path: impl AsRef<Path>) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn validate_asset_name(name: &str) -> Result<(), String> {
    let valid = name.rsplit_once('.').is_some_and(|(hash, extension)| {
        hash.len() == 64
            && hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            && matches!(extension, "png" | "jpg" | "gif" | "webp" | "bin")
    });
    if valid {
        Ok(())
    } else {
        Err(format!("invalid asset filename: {name}"))
    }
}

fn validate_snapshot_path(path: &str) -> Result<(), String> {
    let valid_id = |id: &str| {
        !id.is_empty()
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    };
    let valid = path == "rem.json"
        || path == "tombstones.json"
        || path
            .strip_prefix("decks/")
            .and_then(|name| name.strip_suffix(".json"))
            .is_some_and(valid_id);
    if valid {
        Ok(())
    } else {
        Err(format!("invalid snapshot path: {path}"))
    }
}

fn read_snapshot_file(
    root: &Path,
    relative: &str,
    out: &mut HashMap<String, String>,
) -> Result<(), String> {
    let path = root.join(relative);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if metadata.file_type().is_symlink() {
        return Err(format!("symlink not allowed: {relative}"));
    }
    if !metadata.is_file() {
        return Err(format!("snapshot path is not a file: {relative}"));
    }
    out.insert(
        relative.to_string(),
        fs::read_to_string(path).map_err(|error| error.to_string())?,
    );
    Ok(())
}

/// Read only rem's managed snapshot files without following repository symlinks.
fn collect_files(root: &Path, out: &mut HashMap<String, String>) -> Result<(), String> {
    read_snapshot_file(root, "rem.json", out)?;
    read_snapshot_file(root, "tombstones.json", out)?;

    let decks = root.join("decks");
    let metadata = match fs::symlink_metadata(&decks) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if metadata.file_type().is_symlink() {
        return Err("symlink not allowed: decks".into());
    }
    if !metadata.is_dir() {
        return Err("snapshot path is not a directory: decks".into());
    }

    for entry in fs::read_dir(decks).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        let relative = format!("decks/{name}");
        let metadata = fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err(format!("symlink not allowed: {relative}"));
        }
        if validate_snapshot_path(&relative).is_err() {
            continue;
        }
        if !metadata.is_file() {
            return Err(format!("snapshot path is not a file: {relative}"));
        }
        read_snapshot_file(root, &relative, out)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn git_is_cloned(repo: tauri::State<'_, GitRepo>) -> Result<bool, String> {
    repo.is_cloned().await
}

#[tauri::command]
pub async fn git_clone(repo: tauri::State<'_, GitRepo>, remote_url: String) -> Result<(), String> {
    repo.clone_remote(remote_url).await
}

#[tauri::command]
pub async fn git_set_remote_url(
    repo: tauri::State<'_, GitRepo>,
    remote_url: String,
) -> Result<(), String> {
    repo.set_remote_url(remote_url).await
}

#[tauri::command]
pub async fn git_fetch_reset(repo: tauri::State<'_, GitRepo>) -> Result<bool, String> {
    repo.fetch_reset().await
}

#[tauri::command]
pub async fn git_read_files(
    repo: tauri::State<'_, GitRepo>,
) -> Result<HashMap<String, String>, String> {
    repo.read_files().await
}

#[tauri::command]
pub async fn git_write_files(
    repo: tauri::State<'_, GitRepo>,
    files: HashMap<String, String>,
) -> Result<(), String> {
    repo.write_files(files).await
}

#[tauri::command]
pub async fn git_commit_push(
    repo: tauri::State<'_, GitRepo>,
    message: String,
) -> Result<CommitPushResult, String> {
    repo.commit_push(message).await
}

#[tauri::command]
pub async fn git_read_assets(repo: tauri::State<'_, GitRepo>) -> Result<Vec<AssetFile>, String> {
    repo.read_assets().await
}

#[tauri::command]
pub async fn git_write_assets(
    repo: tauri::State<'_, GitRepo>,
    assets: Vec<AssetFile>,
) -> Result<(), String> {
    repo.write_assets(assets).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn make_temp_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("rem-git-test-{}-{}", std::process::id(), n));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn git_repo(dir: &str) -> GitRepo {
        GitRepo::new(PathBuf::from(dir))
    }

    #[test]
    fn test_git_repo_uses_its_configured_root() {
        let dir = make_temp_dir();
        let repo = GitRepo::new(dir.clone());
        let mut files = HashMap::new();
        files.insert("rem.json".to_string(), r#"{"v":1}"#.to_string());

        tauri::async_runtime::block_on(repo.write_files(files)).unwrap();

        assert_eq!(
            fs::read_to_string(dir.join("rem.json")).unwrap(),
            r#"{"v":1}"#
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_write_files_rejects_traversal_before_mutating() {
        let dir = make_temp_dir();
        let repo = GitRepo::new(dir.clone());
        fs::write(dir.join("rem.json"), "keep").unwrap();
        let outside = dir.parent().unwrap().join("rem-escaped.json");
        let _ = fs::remove_file(&outside);
        let mut files = HashMap::new();
        files.insert("../rem-escaped.json".to_string(), "escaped".to_string());

        let result = tauri::async_runtime::block_on(repo.write_files(files));

        assert_eq!(
            result,
            Err("invalid snapshot path: ../rem-escaped.json".into())
        );
        assert_eq!(fs::read_to_string(dir.join("rem.json")).unwrap(), "keep");
        assert!(!outside.exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_write_files_accepts_only_managed_snapshot_paths() {
        let dir = make_temp_dir();
        let repo = GitRepo::new(dir.clone());
        for path in [
            "notes.txt",
            "decks\\evil.json",
            "decks/bad!.json",
            "decks/nested/id.json",
        ] {
            let mut files = HashMap::new();
            files.insert(path.to_string(), "x".to_string());

            let result = tauri::async_runtime::block_on(repo.write_files(files));

            assert_eq!(result, Err(format!("invalid snapshot path: {path}")));
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn test_read_files_rejects_symlinked_snapshot_file() {
        use std::os::unix::fs::symlink;

        let dir = make_temp_dir();
        let repo = GitRepo::new(dir.clone());
        fs::create_dir_all(dir.join("decks")).unwrap();
        let outside = dir.parent().unwrap().join("rem-outside-deck.json");
        fs::write(&outside, r#"{"secret":true}"#).unwrap();
        symlink(&outside, dir.join("decks/d1.json")).unwrap();

        let result = tauri::async_runtime::block_on(repo.read_files());

        assert_eq!(result, Err("symlink not allowed: decks/d1.json".into()));
        fs::remove_file(&outside).unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn test_write_files_rejects_symlinked_managed_directory() {
        use std::os::unix::fs::symlink;

        let dir = make_temp_dir();
        let repo = GitRepo::new(dir.clone());
        let outside = make_temp_dir();
        symlink(&outside, dir.join("decks")).unwrap();
        let mut files = HashMap::new();
        files.insert("decks/d1.json".to_string(), "escaped".to_string());

        let result = tauri::async_runtime::block_on(repo.write_files(files));

        assert_eq!(result, Err("symlink not allowed: decks".into()));
        assert!(!outside.join("d1.json").exists());
        fs::remove_dir_all(&dir).unwrap();
        fs::remove_dir_all(&outside).unwrap();
    }

    #[test]
    fn test_write_assets_rejects_unsafe_name_before_mutating() {
        let dir = make_temp_dir();
        let repo = GitRepo::new(dir.clone());
        let outside = dir.parent().unwrap().join("rem-asset-escaped.png");
        let _ = fs::remove_file(&outside);
        let assets = vec![AssetFile {
            name: "../../rem-asset-escaped.png".into(),
            data: STANDARD.encode([1_u8, 2, 3]),
        }];

        let result = tauri::async_runtime::block_on(repo.write_assets(assets));

        assert_eq!(
            result,
            Err("invalid asset filename: ../../rem-asset-escaped.png".into())
        );
        assert!(!outside.exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_read_assets_rejects_malformed_filename() {
        let dir = make_temp_dir();
        let repo = GitRepo::new(dir.clone());
        fs::create_dir_all(dir.join("assets")).unwrap();
        fs::write(dir.join("assets/not-a-hash.png"), [1_u8]).unwrap();

        let result = tauri::async_runtime::block_on(repo.read_assets());

        assert_eq!(
            result.err(),
            Some("invalid asset filename: not-a-hash.png".into())
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn test_read_assets_rejects_symlinked_file() {
        use std::os::unix::fs::symlink;

        let dir = make_temp_dir();
        let repo = GitRepo::new(dir.clone());
        fs::create_dir_all(dir.join("assets")).unwrap();
        let outside = dir.parent().unwrap().join("rem-outside-asset.png");
        fs::write(&outside, [1_u8, 2, 3]).unwrap();
        let name = format!("{}.png", "a".repeat(64));
        symlink(&outside, dir.join("assets").join(&name)).unwrap();

        let result = tauri::async_runtime::block_on(repo.read_assets());

        assert_eq!(
            result.err(),
            Some(format!("symlink not allowed: assets/{name}"))
        );
        fs::remove_file(&outside).unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_write_assets_validates_payloads_before_mutating() {
        let dir = make_temp_dir();
        let repo = GitRepo::new(dir.clone());
        fs::create_dir_all(dir.join("assets")).unwrap();
        let existing = format!("{}.png", "a".repeat(64));
        fs::write(dir.join("assets").join(&existing), [9_u8]).unwrap();
        let assets = vec![AssetFile {
            name: format!("{}.png", "b".repeat(64)),
            data: "not-base64".into(),
        }];

        let result = tauri::async_runtime::block_on(repo.write_assets(assets));

        assert!(result.is_err());
        assert_eq!(fs::read(dir.join("assets").join(existing)).unwrap(), [9_u8]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn test_git_repo_rejects_symlinked_root() {
        use std::os::unix::fs::symlink;

        let parent = make_temp_dir();
        let outside = make_temp_dir();
        let root = parent.join("repo");
        symlink(&outside, &root).unwrap();
        let repo = GitRepo::new(root);
        let mut files = HashMap::new();
        files.insert("rem.json".to_string(), "escaped".to_string());

        let result = tauri::async_runtime::block_on(repo.write_files(files));

        assert_eq!(result, Err("repository root must not be a symlink".into()));
        assert!(!outside.join("rem.json").exists());
        fs::remove_dir_all(&parent).unwrap();
        fs::remove_dir_all(&outside).unwrap();
    }

    #[test]
    fn test_write_files_rejects_wrong_managed_type_before_mutating() {
        let dir = make_temp_dir();
        let repo = GitRepo::new(dir.clone());
        fs::write(dir.join("rem.json"), "keep").unwrap();
        fs::write(dir.join("decks"), "not-a-directory").unwrap();
        let mut files = HashMap::new();
        files.insert("decks/d1.json".to_string(), "replacement".to_string());

        let result = tauri::async_runtime::block_on(repo.write_files(files));

        assert_eq!(
            result,
            Err("snapshot path is not a directory: decks".into())
        );
        assert_eq!(fs::read_to_string(dir.join("rem.json")).unwrap(), "keep");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn test_git_repo_rejects_symlinked_git_metadata() {
        use std::os::unix::fs::symlink;

        let dir = make_temp_dir();
        let outside = make_temp_dir();
        symlink(&outside, dir.join(".git")).unwrap();
        let repo = GitRepo::new(dir.clone());

        let result = tauri::async_runtime::block_on(repo.is_cloned());

        assert_eq!(result, Err("repository .git must not be a symlink".into()));
        fs::remove_dir_all(&dir).unwrap();
        fs::remove_dir_all(&outside).unwrap();
    }

    /// Contract #1: git_read_files returns forward-slash, root-relative keys
    /// with no leading "./" and no backslashes.
    #[test]
    fn test_write_read_roundtrip_path_keys() {
        let dir = make_temp_dir();
        let dir_str = dir.to_string_lossy().to_string();

        let mut files = HashMap::new();
        files.insert("rem.json".to_string(), r#"{"v":1}"#.to_string());
        files.insert(
            "decks/abc123.json".to_string(),
            r#"{"id":"abc123"}"#.to_string(),
        );
        files.insert("tombstones.json".to_string(), r#"[]"#.to_string());

        let repo = git_repo(&dir_str);
        tauri::async_runtime::block_on(repo.write_files(files)).unwrap();

        let result = tauri::async_runtime::block_on(repo.read_files()).unwrap();

        assert!(
            result.contains_key("rem.json"),
            "missing rem.json; keys: {:?}",
            result.keys().collect::<Vec<_>>()
        );
        assert!(
            result.contains_key("decks/abc123.json"),
            "missing decks/abc123.json; keys: {:?}",
            result.keys().collect::<Vec<_>>()
        );
        assert!(
            result.contains_key("tombstones.json"),
            "missing tombstones.json; keys: {:?}",
            result.keys().collect::<Vec<_>>()
        );

        // No leading "./"
        for key in result.keys() {
            assert!(!key.starts_with("./"), "key has leading ./: {}", key);
            assert!(!key.contains('\\'), "key has backslash: {}", key);
        }

        fs::remove_dir_all(&dir).unwrap();
    }

    /// Contract #2: git_write_files DELETES files absent from the incoming map.
    #[test]
    fn test_write_files_deletes_absent_files() {
        let dir = make_temp_dir();
        let dir_str = dir.to_string_lossy().to_string();

        // First write: two deck files
        let mut files1 = HashMap::new();
        files1.insert("decks/a.json".to_string(), r#"{"id":"a"}"#.to_string());
        files1.insert("decks/b.json".to_string(), r#"{"id":"b"}"#.to_string());
        let repo = git_repo(&dir_str);
        tauri::async_runtime::block_on(repo.write_files(files1)).unwrap();

        // Second write: only deck a, no deck b
        let mut files2 = HashMap::new();
        files2.insert("decks/a.json".to_string(), r#"{"id":"a"}"#.to_string());
        tauri::async_runtime::block_on(repo.write_files(files2)).unwrap();

        let result = tauri::async_runtime::block_on(repo.read_files()).unwrap();

        assert!(
            result.contains_key("decks/a.json"),
            "decks/a.json should be present"
        );
        assert!(
            !result.contains_key("decks/b.json"),
            "decks/b.json should have been deleted"
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    /// Helper: run a plain git command for fixture setup; panics on failure.
    fn fixture_git(args: &[&str], dir: &str) {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git not found");
        if !out.status.success() {
            panic!(
                "fixture git {:?} failed in {}:\n{}",
                args,
                dir,
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    #[test]
    fn test_set_remote_url_updates_origin() {
        let root = make_temp_dir();
        let root_str = root.to_string_lossy().to_string();
        let origin1 = root.join("origin1.git");
        let origin2 = root.join("origin2.git");
        let clone = root.join("clone");
        let origin1_str = origin1.to_string_lossy().to_string();
        let origin2_str = origin2.to_string_lossy().to_string();
        let clone_str = clone.to_string_lossy().to_string();

        fixture_git(&["init", "--bare", &origin1_str], &root_str);
        fixture_git(&["init", "--bare", &origin2_str], &root_str);
        fixture_git(&["clone", &origin1_str, &clone_str], &root_str);

        let repo = git_repo(&clone_str);
        tauri::async_runtime::block_on(repo.set_remote_url(origin2_str.clone())).unwrap();
        let (stdout, _, success) =
            run_git(&["remote", "get-url", "origin"], Path::new(&clone_str)).unwrap();
        assert!(success);
        assert_eq!(stdout.trim(), origin2_str);

        let _ = fs::remove_dir_all(&root);
    }

    /// Contract #3 + first push: git_fetch_reset returns Ok(false) for an empty remote,
    /// then Ok(true) after git_commit_push creates origin/main.
    #[test]
    fn test_fetch_reset_empty_vs_populated() {
        let root = std::env::temp_dir().join(format!("rem-git-it-{}-0", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let root_str = root.to_string_lossy().to_string();

        let origin = root.join("origin.git");
        let clone1 = root.join("clone1");
        let origin_str = origin.to_string_lossy().to_string();
        let clone1_str = clone1.to_string_lossy().to_string();

        // 1. Bare repo as the remote.
        fixture_git(&["init", "--bare", &origin_str], &root_str);

        // 2. Clone the empty bare repo (prints a harmless warning, that's fine).
        fixture_git(&["clone", &origin_str, &clone1_str], &root_str);

        let repo1 = git_repo(&clone1_str);

        // 3. fetch_reset on an empty remote → Ok(false).
        let result = tauri::async_runtime::block_on(repo1.fetch_reset());
        assert_eq!(
            result,
            Ok(false),
            "expected Ok(false) for empty remote, got {:?}",
            result
        );

        // 4. Write the required files.
        let mut files = HashMap::new();
        files.insert("rem.json".to_string(), "{}".to_string());
        files.insert("decks/a.json".to_string(), "{}".to_string());
        files.insert("tombstones.json".to_string(), "[]".to_string());
        tauri::async_runtime::block_on(repo1.write_files(files)).unwrap();

        // 5. First push creates origin/main.
        let push_result =
            tauri::async_runtime::block_on(repo1.commit_push("first".to_string())).unwrap();
        assert!(push_result.pushed, "expected pushed==true on first commit");
        assert!(
            !push_result.rejected,
            "expected rejected==false on first commit"
        );

        // 6. fetch_reset now sees origin/main → Ok(true).
        let result2 = tauri::async_runtime::block_on(repo1.fetch_reset());
        assert_eq!(
            result2,
            Ok(true),
            "expected Ok(true) after first push, got {:?}",
            result2
        );

        // 7. git_read_files contains decks/a.json.
        let files_read = tauri::async_runtime::block_on(repo1.read_files()).unwrap();
        assert!(
            files_read.contains_key("decks/a.json"),
            "expected decks/a.json in read result; keys: {:?}",
            files_read.keys().collect::<Vec<_>>()
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// Contract #4: non-fast-forward push returns pushed==false, rejected==true;
    /// after fetch_reset the retry succeeds.
    #[test]
    fn test_commit_push_non_fast_forward() {
        let root = std::env::temp_dir().join(format!("rem-git-it-{}-1", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let root_str = root.to_string_lossy().to_string();

        let origin = root.join("origin.git");
        let clone1 = root.join("clone1");
        let clone2 = root.join("clone2");
        let origin_str = origin.to_string_lossy().to_string();
        let clone1_str = clone1.to_string_lossy().to_string();
        let clone2_str = clone2.to_string_lossy().to_string();

        // 1. Bare remote + clone1.
        fixture_git(&["init", "--bare", &origin_str], &root_str);
        fixture_git(&["clone", &origin_str, &clone1_str], &root_str);

        // 2. Seed origin via clone1 (first commit).
        let mut seed_files = HashMap::new();
        seed_files.insert("rem.json".to_string(), "{}".to_string());
        seed_files.insert("tombstones.json".to_string(), "[]".to_string());
        let repo1 = git_repo(&clone1_str);
        tauri::async_runtime::block_on(repo1.write_files(seed_files)).unwrap();
        let r = tauri::async_runtime::block_on(repo1.commit_push("first".to_string())).unwrap();
        assert!(r.pushed, "seed push from clone1 should succeed");

        // 3. clone2 gets the first commit.
        fixture_git(&["clone", &origin_str, &clone2_str], &root_str);

        // 4. clone2 advances origin/main.
        let repo2 = git_repo(&clone2_str);
        let result_fetch2 = tauri::async_runtime::block_on(repo2.fetch_reset());
        assert_eq!(
            result_fetch2,
            Ok(true),
            "clone2 fetch_reset should see origin/main"
        );
        let mut files2 = HashMap::new();
        files2.insert("rem.json".to_string(), "{}".to_string());
        files2.insert("decks/b.json".to_string(), "{}".to_string());
        files2.insert("tombstones.json".to_string(), "[]".to_string());
        tauri::async_runtime::block_on(repo2.write_files(files2)).unwrap();
        let r2 = tauri::async_runtime::block_on(repo2.commit_push("from2".to_string())).unwrap();
        assert!(r2.pushed, "clone2 push should be a fast-forward");

        // 5. clone1 is behind; its push must be rejected (non-fast-forward).
        let mut files1 = HashMap::new();
        files1.insert("rem.json".to_string(), "{}".to_string());
        files1.insert("decks/c.json".to_string(), "{}".to_string());
        files1.insert("tombstones.json".to_string(), "[]".to_string());
        tauri::async_runtime::block_on(repo1.write_files(files1.clone())).unwrap();
        let r_rejected =
            tauri::async_runtime::block_on(repo1.commit_push("from1".to_string())).unwrap();
        assert!(
            !r_rejected.pushed && r_rejected.rejected,
            "expected pushed==false, rejected==true; got {:?}/{:?}",
            r_rejected.pushed,
            r_rejected.rejected
        );

        // 6. Retry: fetch_reset resets clone1 to clone2's state, then push succeeds.
        let result_retry_fetch = tauri::async_runtime::block_on(repo1.fetch_reset());
        assert_eq!(
            result_retry_fetch,
            Ok(true),
            "retry fetch_reset should see origin/main"
        );
        tauri::async_runtime::block_on(repo1.write_files(files1)).unwrap();
        let r_retry =
            tauri::async_runtime::block_on(repo1.commit_push("from1-retry".to_string())).unwrap();
        assert!(
            r_retry.pushed && !r_retry.rejected,
            "retry push should succeed; got pushed={} rejected={}",
            r_retry.pushed,
            r_retry.rejected
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_write_read_assets_binary_roundtrip_and_delete_absent() {
        let dir = make_temp_dir();
        let dir_str = dir.to_string_lossy().to_string();

        // Non-UTF8 bytes must survive the round trip.
        let bytes = vec![0u8, 1, 254, 255, 128];
        let b64 = STANDARD.encode(&bytes);
        let png_name = format!("{}.png", "a".repeat(64));
        let gif_name = format!("{}.gif", "b".repeat(64));
        let assets = vec![
            AssetFile {
                name: png_name.clone(),
                data: b64.clone(),
            },
            AssetFile {
                name: gif_name,
                data: b64.clone(),
            },
        ];
        let repo = git_repo(&dir_str);
        tauri::async_runtime::block_on(repo.write_assets(assets)).unwrap();

        let read = tauri::async_runtime::block_on(repo.read_assets()).unwrap();
        assert_eq!(read.len(), 2);
        let png = read.iter().find(|asset| asset.name == png_name).unwrap();
        assert_eq!(STANDARD.decode(&png.data).unwrap(), bytes);

        // Second write with only one asset deletes the other.
        tauri::async_runtime::block_on(repo.write_assets(vec![AssetFile {
            name: png_name.clone(),
            data: b64,
        }]))
        .unwrap();
        let read2 = tauri::async_runtime::block_on(repo.read_assets()).unwrap();
        assert_eq!(read2.len(), 1);
        assert_eq!(read2[0].name, png_name);

        fs::remove_dir_all(&dir).unwrap();
    }

    /// Contract #6: git_read_files must NOT read assets/ (binary files break UTF-8).
    /// After writing a text file AND a binary asset, read_files returns Ok, contains
    /// the text key, and contains NO assets/... key.
    #[test]
    fn test_read_files_skips_assets_dir() {
        let dir = make_temp_dir();
        let dir_str = dir.to_string_lossy().to_string();

        // 1. Write a normal text file.
        let mut files = HashMap::new();
        files.insert("decks/a.json".to_string(), r#"{"id":"a"}"#.to_string());
        let repo = git_repo(&dir_str);
        tauri::async_runtime::block_on(repo.write_files(files)).unwrap();

        // 2. Write a binary asset with non-UTF-8 bytes.
        let bad_bytes = vec![0u8, 1, 254, 255, 128];
        let assets = vec![AssetFile {
            name: format!("{}.png", "a".repeat(64)),
            data: STANDARD.encode(&bad_bytes),
        }];
        tauri::async_runtime::block_on(repo.write_assets(assets)).unwrap();

        // 3. git_read_files must return Ok (not Err from trying to UTF-8 decode the PNG).
        let result = tauri::async_runtime::block_on(repo.read_files());
        assert!(result.is_ok(), "git_read_files failed: {:?}", result.err());
        let map = result.unwrap();

        // 4. Text file is present.
        assert!(
            map.contains_key("decks/a.json"),
            "expected decks/a.json; keys: {:?}",
            map.keys().collect::<Vec<_>>()
        );

        // 5. No assets/ key leaked through.
        let asset_keys: Vec<_> = map.keys().filter(|k| k.starts_with("assets/")).collect();
        assert!(
            asset_keys.is_empty(),
            "git_read_files should not include assets/ keys; found: {:?}",
            asset_keys
        );

        // 6. git_read_assets still returns the asset correctly.
        let assets_read = tauri::async_runtime::block_on(repo.read_assets()).unwrap();
        assert_eq!(assets_read.len(), 1);
        assert_eq!(STANDARD.decode(&assets_read[0].data).unwrap(), bad_bytes);

        fs::remove_dir_all(&dir).unwrap();
    }
}
