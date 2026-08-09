mod fsrs_sched;
mod git;
mod storage;
mod study;

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
            study::study_start,
            study::study_reveal,
            study::study_grade,
            study::study_advance_preview,
            study::study_end,
            storage::storage_create_deck,
            storage::storage_list_decks,
            storage::storage_get_deck,
            storage::storage_delete_deck,
            storage::storage_update_deck,
            storage::storage_create_card,
            storage::storage_propose_drafts,
            storage::storage_list_drafts,
            storage::storage_resolve_draft,
            storage::storage_get_card,
            storage::storage_list_cards,
            storage::storage_update_card,
            storage::storage_delete_card,
            storage::storage_commit_review,
            storage::storage_list_review_logs,
            storage::storage_due_cards,
            storage::storage_count_due,
            storage::storage_get_daily_stat,
            storage::storage_import_decks,
            storage::storage_export_snapshot,
            storage::storage_apply_merge,
            storage::storage_put_asset,
            storage::storage_get_asset,
            storage::storage_sweep_orphan_assets,
        ])
        .setup(|app| {
            let database_path = rem_core::default_database_path()
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            let collection = rem_core::Collection::open(database_path)
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            app.manage(collection);
            app.manage(study::StudySessions::default());

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
