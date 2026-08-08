mod fsrs_sched;
mod git;

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
