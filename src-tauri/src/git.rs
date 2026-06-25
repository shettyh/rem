use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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

#[tauri::command]
pub async fn git_is_cloned(dir: String) -> Result<bool, String> {
    Ok(Path::new(&dir).join(".git").exists())
}

#[tauri::command]
pub async fn git_clone(remote_url: String, dir: String) -> Result<(), String> {
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
pub async fn git_fetch_reset(dir: String) -> Result<bool, String> {
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
pub async fn git_read_files(dir: String) -> Result<HashMap<String, String>, String> {
    let root = PathBuf::from(&dir);
    let mut out = HashMap::new();
    collect_files(&root, &root, &mut out)?;
    Ok(out)
}

#[tauri::command]
pub async fn git_write_files(dir: String, files: HashMap<String, String>) -> Result<(), String> {
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
pub async fn git_commit_push(dir: String, message: String) -> Result<CommitPushResult, String> {
    ok_or_stderr(run_git(&["add", "-A"], &dir)?)?;
    // Commit only if something is staged. Identity passed inline so commits work
    // even without global git config.
    let (_, _, has_changes) = run_git(&["diff", "--cached", "--quiet"], &dir)?;
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
            &dir,
        )?)?;
    }
    // Push to main; classify non-fast-forward as a (retryable) rejection.
    let (_, stderr, success) = run_git(&["push", "origin", "HEAD:main"], &dir)?;
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

        git_write_files(dir_str.clone(), files).unwrap();

        let result = git_read_files(dir_str).unwrap();

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
        git_write_files(dir_str.clone(), files1).unwrap();

        // Second write: only deck a, no deck b
        let mut files2 = HashMap::new();
        files2.insert("decks/a.json".to_string(), r#"{"id":"a"}"#.to_string());
        git_write_files(dir_str.clone(), files2).unwrap();

        let result = git_read_files(dir_str).unwrap();

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

        // 3. fetch_reset on an empty remote → Ok(false).
        let result = git_fetch_reset(clone1_str.clone());
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
        git_write_files(clone1_str.clone(), files).unwrap();

        // 5. First push creates origin/main.
        let push_result = git_commit_push(clone1_str.clone(), "first".to_string()).unwrap();
        assert!(push_result.pushed, "expected pushed==true on first commit");
        assert!(
            !push_result.rejected,
            "expected rejected==false on first commit"
        );

        // 6. fetch_reset now sees origin/main → Ok(true).
        let result2 = git_fetch_reset(clone1_str.clone());
        assert_eq!(
            result2,
            Ok(true),
            "expected Ok(true) after first push, got {:?}",
            result2
        );

        // 7. git_read_files contains decks/a.json.
        let files_read = git_read_files(clone1_str).unwrap();
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
        git_write_files(clone1_str.clone(), seed_files).unwrap();
        let r = git_commit_push(clone1_str.clone(), "first".to_string()).unwrap();
        assert!(r.pushed, "seed push from clone1 should succeed");

        // 3. clone2 gets the first commit.
        fixture_git(&["clone", &origin_str, &clone2_str], &root_str);

        // 4. clone2 advances origin/main.
        let result_fetch2 = git_fetch_reset(clone2_str.clone());
        assert_eq!(
            result_fetch2,
            Ok(true),
            "clone2 fetch_reset should see origin/main"
        );
        let mut files2 = HashMap::new();
        files2.insert("rem.json".to_string(), "{}".to_string());
        files2.insert("decks/b.json".to_string(), "{}".to_string());
        files2.insert("tombstones.json".to_string(), "[]".to_string());
        git_write_files(clone2_str.clone(), files2).unwrap();
        let r2 = git_commit_push(clone2_str.clone(), "from2".to_string()).unwrap();
        assert!(r2.pushed, "clone2 push should be a fast-forward");

        // 5. clone1 is behind; its push must be rejected (non-fast-forward).
        let mut files1 = HashMap::new();
        files1.insert("rem.json".to_string(), "{}".to_string());
        files1.insert("decks/c.json".to_string(), "{}".to_string());
        files1.insert("tombstones.json".to_string(), "[]".to_string());
        git_write_files(clone1_str.clone(), files1.clone()).unwrap();
        let r_rejected = git_commit_push(clone1_str.clone(), "from1".to_string()).unwrap();
        assert!(
            !r_rejected.pushed && r_rejected.rejected,
            "expected pushed==false, rejected==true; got {:?}/{:?}",
            r_rejected.pushed,
            r_rejected.rejected
        );

        // 6. Retry: fetch_reset resets clone1 to clone2's state, then push succeeds.
        let result_retry_fetch = git_fetch_reset(clone1_str.clone());
        assert_eq!(
            result_retry_fetch,
            Ok(true),
            "retry fetch_reset should see origin/main"
        );
        git_write_files(clone1_str.clone(), files1).unwrap();
        let r_retry = git_commit_push(clone1_str.clone(), "from1-retry".to_string()).unwrap();
        assert!(
            r_retry.pushed && !r_retry.rejected,
            "retry push should succeed; got pushed={} rejected={}",
            r_retry.pushed,
            r_retry.rejected
        );

        let _ = fs::remove_dir_all(&root);
    }
}
