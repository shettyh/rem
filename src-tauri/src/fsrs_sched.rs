use rem_core::{optimize_fsrs_histories, FsrsReviewHistory};

#[tauri::command]
pub async fn fsrs_optimize(
    histories: Vec<FsrsReviewHistory>,
    num_relearning_steps: usize,
) -> Result<Vec<f32>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        optimize_fsrs_histories(histories, num_relearning_steps)
    })
    .await
    .map_err(|error| error.to_string())?
}
