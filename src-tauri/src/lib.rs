mod fsrs_sched;
mod git;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            git::git_is_cloned,
            git::git_clone,
            git::git_set_remote_url,
            git::git_fetch_reset,
            git::git_read_files,
            git::git_write_files,
            git::git_read_assets,
            git::git_write_assets,
            git::git_commit_push,
            fsrs_sched::fsrs_next_states,
            fsrs_sched::fsrs_optimize,
        ])
        .setup(|app| {
            let repo_dir = app.path().app_data_dir()?.join("repo");
            app.manage(git::GitRepo::new(repo_dir));
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    #[test]
    fn packaged_webview_has_a_restrictive_csp() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let csp = config["app"]["security"]["csp"].as_object().unwrap();

        assert_eq!(csp["default-src"], "'self'");
        assert_eq!(csp["connect-src"], "'self' ipc: http://ipc.localhost");
        assert_eq!(csp["img-src"], "'self' blob: data:");
        assert_eq!(csp["object-src"], "'none'");
        assert_eq!(csp["base-uri"], "'none'");
        assert!(!csp["script-src"].as_str().unwrap().contains("unsafe"));
    }
}
