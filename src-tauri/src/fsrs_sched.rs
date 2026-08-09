use rem_core::{
    calculate_fsrs_next_states, optimize_fsrs_histories, DeckFsrsParams, FsrsNextStates,
    FsrsReviewHistory, FsrsState,
};

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

#[tauri::command]
pub fn fsrs_next_states(
    state: FsrsState,
    now: i64,
    params: DeckFsrsParams,
) -> Result<FsrsNextStates, String> {
    calculate_fsrs_next_states(&state, now, &params)
}
