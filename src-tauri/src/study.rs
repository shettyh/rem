use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use rem_core::{Collection, Grade, StudyGradeOutcome, StudyRequest, StudySession, StudyView};
use serde::Serialize;
use tauri::State;

#[derive(Debug, Default)]
pub struct StudySessions {
    next_id: AtomicU64,
    sessions: Mutex<HashMap<String, StudySession>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartedStudy {
    session_id: String,
    view: StudyView,
}

#[tauri::command]
pub fn study_start(
    collection: State<'_, Collection>,
    sessions: State<'_, StudySessions>,
    request: StudyRequest,
    now: i64,
) -> Result<StartedStudy, String> {
    let session =
        StudySession::start(&collection, request, now).map_err(|error| error.to_string())?;
    let view = session.view();
    let session_id = format!(
        "study-{}",
        sessions.next_id.fetch_add(1, Ordering::Relaxed) + 1
    );
    sessions
        .sessions
        .lock()
        .map_err(|_| "study sessions are unavailable".to_owned())?
        .insert(session_id.clone(), session);
    Ok(StartedStudy { session_id, view })
}

#[tauri::command]
pub fn study_reveal(
    sessions: State<'_, StudySessions>,
    session_id: String,
    now: i64,
) -> Result<StudyView, String> {
    with_session(&sessions, &session_id, |session| {
        session.reveal(now).map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub fn study_grade(
    collection: State<'_, Collection>,
    sessions: State<'_, StudySessions>,
    session_id: String,
    grade: Grade,
    now: i64,
) -> Result<StudyGradeOutcome, String> {
    with_session(&sessions, &session_id, |session| {
        session
            .grade(&collection, grade, now)
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub fn study_advance_preview(
    sessions: State<'_, StudySessions>,
    session_id: String,
    now: i64,
) -> Result<StudyView, String> {
    with_session(&sessions, &session_id, |session| {
        session
            .advance_preview(now)
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub fn study_end(sessions: State<'_, StudySessions>, session_id: String) -> Result<(), String> {
    sessions
        .sessions
        .lock()
        .map_err(|_| "study sessions are unavailable".to_owned())?
        .remove(&session_id);
    Ok(())
}

fn with_session<T>(
    sessions: &StudySessions,
    session_id: &str,
    operation: impl FnOnce(&mut StudySession) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = sessions
        .sessions
        .lock()
        .map_err(|_| "study sessions are unavailable".to_owned())?;
    let session = guard
        .get_mut(session_id)
        .ok_or_else(|| format!("study session not found: {session_id}"))?;
    operation(session)
}
