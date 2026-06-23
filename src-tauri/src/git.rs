use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

#[derive(Serialize)]
pub struct CommitPushResult {
    pushed: bool,
    rejected: bool,
}

/// Run git in `dir`, returning (stdout, stderr, success). Maps a missing git
/// binary to a recognizable error string.
fn run_git(args: &[&str], dir: &str) -> Result<(String, String, bool), String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
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
    if success { Ok(stdout) } else { Err(stderr) }
}

#[tauri::command]
pub fn git_is_cloned(dir: String) -> Result<bool, String> {
    Ok(Path::new(&dir).join(".git").exists())
}

#[tauri::command]
pub fn git_clone(remote_url: String, dir: String) -> Result<(), String> {
    if let Some(parent) = Path::new(&dir).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Clone into `dir` (its parent must exist; `dir` itself is created by git).
    let parent = Path::new(&dir)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());
    let name = Path::new(&dir)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or("invalid repo dir")?;
    ok_or_stderr(run_git(&["clone", &remote_url, &name], &parent)?)?;
    Ok(())
}

#[tauri::command]
pub fn git_fetch_reset(dir: String) -> Result<bool, String> {
    ok_or_stderr(run_git(&["fetch", "origin"], &dir)?)?;
    let (_, _, has_main) = run_git(&["rev-parse", "--verify", "origin/main"], &dir)?;
    if !has_main {
        return Ok(false); // empty remote: no main branch yet
    }
    ok_or_stderr(run_git(&["reset", "--hard", "origin/main"], &dir)?)?;
    Ok(true)
}

/// Recursively collect tracked content files (decks/, rem.json, tombstones.json)
/// as a path->content map, with forward-slash relative paths.
fn collect_files(root: &Path, dir: &Path, out: &mut HashMap<String, String>) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name == ".git" {
            continue;
        }
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            out.insert(rel, content);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn git_read_files(dir: String) -> Result<HashMap<String, String>, String> {
    let root = PathBuf::from(&dir);
    let mut out = HashMap::new();
    collect_files(&root, &root, &mut out)?;
    Ok(out)
}

#[tauri::command]
pub fn git_write_files(dir: String, files: HashMap<String, String>) -> Result<(), String> {
    let root = PathBuf::from(&dir);
    // Clear the managed set so deletions take effect, then write incoming files.
    let _ = fs::remove_dir_all(root.join("decks"));
    let _ = fs::remove_file(root.join("tombstones.json"));
    let _ = fs::remove_file(root.join("rem.json"));
    for (rel, content) in files {
        let full = root.join(&rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&full, content).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn git_commit_push(dir: String, message: String) -> Result<CommitPushResult, String> {
    ok_or_stderr(run_git(&["add", "-A"], &dir)?)?;
    // Commit only if something is staged. Identity passed inline so commits work
    // even without global git config.
    let (_, _, has_changes) = run_git(&["diff", "--cached", "--quiet"], &dir)?;
    if !has_changes {
        ok_or_stderr(run_git(
            &[
                "-c", "user.name=rem", "-c", "user.email=rem@localhost",
                "commit", "-m", &message,
            ],
            &dir,
        )?)?;
    }
    // Push to main; classify non-fast-forward as a (retryable) rejection.
    let (_, stderr, success) = run_git(&["push", "origin", "HEAD:main"], &dir)?;
    if success {
        return Ok(CommitPushResult { pushed: true, rejected: false });
    }
    let lower = stderr.to_lowercase();
    if lower.contains("rejected") || lower.contains("non-fast-forward") || lower.contains("fetch first") {
        return Ok(CommitPushResult { pushed: false, rejected: true });
    }
    Err(stderr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn make_temp_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "rem-git-test-{}-{}",
            std::process::id(),
            n
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Contract #1: git_read_files returns forward-slash, root-relative keys
    /// with no leading "./" and no backslashes.
    #[test]
    fn test_write_read_roundtrip_path_keys() {
        let dir = make_temp_dir();
        let dir_str = dir.to_string_lossy().to_string();

        let mut files = HashMap::new();
        files.insert("rem.json".to_string(), r#"{"v":1}"#.to_string());
        files.insert("decks/abc123.json".to_string(), r#"{"id":"abc123"}"#.to_string());
        files.insert("tombstones.json".to_string(), r#"[]"#.to_string());

        git_write_files(dir_str.clone(), files).unwrap();

        let result = git_read_files(dir_str).unwrap();

        assert!(result.contains_key("rem.json"), "missing rem.json; keys: {:?}", result.keys().collect::<Vec<_>>());
        assert!(result.contains_key("decks/abc123.json"), "missing decks/abc123.json; keys: {:?}", result.keys().collect::<Vec<_>>());
        assert!(result.contains_key("tombstones.json"), "missing tombstones.json; keys: {:?}", result.keys().collect::<Vec<_>>());

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
        git_write_files(dir_str.clone(), files1).unwrap();

        // Second write: only deck a, no deck b
        let mut files2 = HashMap::new();
        files2.insert("decks/a.json".to_string(), r#"{"id":"a"}"#.to_string());
        git_write_files(dir_str.clone(), files2).unwrap();

        let result = git_read_files(dir_str).unwrap();

        assert!(result.contains_key("decks/a.json"), "decks/a.json should be present");
        assert!(!result.contains_key("decks/b.json"), "decks/b.json should have been deleted");

        fs::remove_dir_all(&dir).unwrap();
    }
}
