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

impl StudySessions {
    fn start(
        &self,
        collection: &Collection,
        request: StudyRequest,
        now: i64,
    ) -> Result<StartedStudy, String> {
        let session =
            StudySession::start(collection, request, now).map_err(|error| error.to_string())?;
        let view = session.view();
        let session_id = format!("study-{}", self.next_id.fetch_add(1, Ordering::Relaxed) + 1);
        self.sessions
            .lock()
            .map_err(|_| "study sessions are unavailable".to_owned())?
            .insert(session_id.clone(), session);
        Ok(StartedStudy { session_id, view })
    }

    fn reveal(&self, session_id: &str, now: i64) -> Result<StudyView, String> {
        with_session(self, session_id, |session| {
            session.reveal(now).map_err(|error| error.to_string())
        })
    }

    fn grade(
        &self,
        collection: &Collection,
        session_id: &str,
        grade: Grade,
        now: i64,
    ) -> Result<StudyGradeOutcome, String> {
        with_session(self, session_id, |session| {
            session
                .grade(collection, grade, now)
                .map_err(|error| error.to_string())
        })
    }

    fn advance_preview(&self, session_id: &str, now: i64) -> Result<StudyView, String> {
        with_session(self, session_id, |session| {
            session
                .advance_preview(now)
                .map_err(|error| error.to_string())
        })
    }

    fn undo_last_grade(
        &self,
        collection: &Collection,
        session_id: &str,
        now: i64,
    ) -> Result<StudyView, String> {
        with_session(self, session_id, |session| {
            session
                .undo_last_grade(collection, now)
                .map_err(|error| error.to_string())
        })
    }

    fn suspend_current(
        &self,
        collection: &Collection,
        session_id: &str,
        now: i64,
    ) -> Result<StudyView, String> {
        with_session(self, session_id, |session| {
            session
                .suspend_current(collection, now)
                .map_err(|error| error.to_string())
        })
    }

    fn end(&self, session_id: &str) -> Result<(), String> {
        self.sessions
            .lock()
            .map_err(|_| "study sessions are unavailable".to_owned())?
            .remove(session_id);
        Ok(())
    }
}

#[tauri::command]
pub fn study_start(
    collection: State<'_, Collection>,
    sessions: State<'_, StudySessions>,
    request: StudyRequest,
    now: i64,
) -> Result<StartedStudy, String> {
    sessions.start(&collection, request, now)
}

#[tauri::command]
pub fn study_reveal(
    sessions: State<'_, StudySessions>,
    session_id: String,
    now: i64,
) -> Result<StudyView, String> {
    sessions.reveal(&session_id, now)
}

#[tauri::command]
pub fn study_grade(
    collection: State<'_, Collection>,
    sessions: State<'_, StudySessions>,
    session_id: String,
    grade: Grade,
    now: i64,
) -> Result<StudyGradeOutcome, String> {
    sessions.grade(&collection, &session_id, grade, now)
}

#[tauri::command]
pub fn study_advance_preview(
    sessions: State<'_, StudySessions>,
    session_id: String,
    now: i64,
) -> Result<StudyView, String> {
    sessions.advance_preview(&session_id, now)
}

#[tauri::command]
pub fn study_undo_last_grade(
    collection: State<'_, Collection>,
    sessions: State<'_, StudySessions>,
    session_id: String,
    now: i64,
) -> Result<StudyView, String> {
    sessions.undo_last_grade(&collection, &session_id, now)
}

#[tauri::command]
pub fn study_suspend_current(
    collection: State<'_, Collection>,
    sessions: State<'_, StudySessions>,
    session_id: String,
    now: i64,
) -> Result<StudyView, String> {
    sessions.suspend_current(&collection, &session_id, now)
}

#[tauri::command]
pub fn study_end(sessions: State<'_, StudySessions>, session_id: String) -> Result<(), String> {
    sessions.end(&session_id)
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

#[cfg(test)]
mod tests {
    use super::*;
    use rem_core::{CustomStudyMode, CustomStudyRequest};
    use std::path::PathBuf;
    use std::sync::Arc;

    const NOW: i64 = 1_760_000_000_000;

    struct TempCollection {
        dir: PathBuf,
        collection: Collection,
    }

    impl TempCollection {
        fn new(label: &str) -> Self {
            static NEXT_TMP: AtomicU64 = AtomicU64::new(0);
            let id = NEXT_TMP.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "rem-study-test-{label}-{}-{id}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let collection = Collection::open(dir.join("collection.sqlite3")).unwrap();
            Self { dir, collection }
        }
    }

    impl Drop for TempCollection {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn study_sessions_support_full_review_undo_suspend_and_end_lifecycle() {
        let tmp = TempCollection::new("lifecycle");
        let deck = tmp.collection.create_deck("Rust", NOW).unwrap();
        let card1 = tmp
            .collection
            .create_card(&deck.id, "Q1", "A1", vec!["core".into()], NOW)
            .unwrap();
        let card2 = tmp
            .collection
            .create_card(&deck.id, "Q2", "A2", vec![], NOW + 1)
            .unwrap();

        let sessions = StudySessions::default();
        let started = sessions
            .start(
                &tmp.collection,
                StudyRequest::deck(deck.id.clone()),
                NOW + 10,
            )
            .unwrap();
        assert_eq!(started.session_id, "study-1");
        assert!(!started.view.revealed);
        assert_eq!(started.view.remaining, 2);
        assert_eq!(started.view.reviewed, 0);
        assert_eq!(
            started.view.current.as_ref().map(|c| c.id.as_str()),
            Some(card1.id.as_str())
        );

        // Wire JSON shape matches TypeScript StartedStudy (`sessionId`, `view`).
        let wire = serde_json::to_value(&started).unwrap();
        assert_eq!(wire["sessionId"], "study-1");
        assert_eq!(wire["view"]["revealed"], false);
        assert_eq!(wire["view"]["remaining"], 2);
        assert_eq!(wire["view"]["canUndo"], false);

        // Grading before reveal is rejected.
        let err = sessions
            .grade(&tmp.collection, &started.session_id, Grade::Good, NOW + 20)
            .unwrap_err();
        assert!(err.contains("revealed"), "unexpected error: {err}");

        // Reveal exposes nextStates.
        let revealed = sessions.reveal(&started.session_id, NOW + 20).unwrap();
        assert!(revealed.revealed);
        assert!(revealed.next_states.is_some());

        // Grade Easy graduates card1 out of the queue and records a review log.
        let outcome = sessions
            .grade(&tmp.collection, &started.session_id, Grade::Easy, NOW + 30)
            .unwrap();
        match outcome {
            StudyGradeOutcome::Graded { view } => {
                assert_eq!(view.reviewed, 1);
                assert_eq!(view.remaining, 1);
                assert!(view.can_undo);
                assert_eq!(
                    view.current.as_ref().map(|c| c.id.as_str()),
                    Some(card2.id.as_str())
                );
            }
            other => panic!("expected Graded outcome, got {other:?}"),
        }
        assert_eq!(tmp.collection.list_review_logs(&deck.id).unwrap().len(), 1);

        // Undo restores card1 as current and deletes the review log.
        let undone = sessions
            .undo_last_grade(&tmp.collection, &started.session_id, NOW + 40)
            .unwrap();
        assert_eq!(undone.reviewed, 0);
        assert_eq!(undone.remaining, 2);
        assert!(!undone.can_undo);
        assert_eq!(
            undone.current.as_ref().map(|c| c.id.as_str()),
            Some(card1.id.as_str())
        );
        assert!(tmp
            .collection
            .list_review_logs(&deck.id)
            .unwrap()
            .is_empty());

        // Suspend marks card1 suspended in the collection and advances to card2.
        let after_suspend = sessions
            .suspend_current(&tmp.collection, &started.session_id, NOW + 50)
            .unwrap();
        assert_eq!(after_suspend.remaining, 1);
        assert_eq!(
            after_suspend.current.as_ref().map(|c| c.id.as_str()),
            Some(card2.id.as_str())
        );
        assert!(
            tmp.collection
                .get_card(&card1.id)
                .unwrap()
                .unwrap()
                .suspended
        );

        // End drops the session so subsequent operations report not found.
        sessions.end(&started.session_id).unwrap();
        let err = sessions.reveal(&started.session_id, NOW + 60).unwrap_err();
        assert_eq!(err, "study session not found: study-1");
    }

    #[test]
    fn study_sessions_support_preview_advance_and_reject_grading_preview() {
        let tmp = TempCollection::new("preview");
        let deck = tmp.collection.create_deck("PreviewDeck", NOW).unwrap();
        tmp.collection
            .create_card(&deck.id, "P1", "A1", vec![], NOW)
            .unwrap();
        tmp.collection
            .create_card(&deck.id, "P2", "A2", vec![], NOW + 1)
            .unwrap();

        let sessions = StudySessions::default();
        let started = sessions
            .start(
                &tmp.collection,
                StudyRequest {
                    deck_id: Some(deck.id.clone()),
                    custom: Some(CustomStudyRequest {
                        mode: CustomStudyMode::PreviewNew,
                        amount: 7,
                    }),
                },
                NOW + 10,
            )
            .unwrap();
        assert!(started.view.preview);
        assert_eq!(started.view.remaining, 2);

        // Advancing before reveal is rejected.
        let err = sessions
            .advance_preview(&started.session_id, NOW + 15)
            .unwrap_err();
        assert!(err.contains("revealed"), "unexpected error: {err}");

        // Reveal in preview mode leaves next_states empty.
        let revealed = sessions.reveal(&started.session_id, NOW + 20).unwrap();
        assert!(revealed.revealed);
        assert!(revealed.next_states.is_none());

        // Grading in preview mode is rejected.
        let err = sessions
            .grade(&tmp.collection, &started.session_id, Grade::Good, NOW + 25)
            .unwrap_err();
        assert!(err.contains("preview"), "unexpected error: {err}");

        // Advance moves to the second card without creating review logs.
        let next = sessions
            .advance_preview(&started.session_id, NOW + 30)
            .unwrap();
        assert_eq!(next.reviewed, 1);
        assert_eq!(next.remaining, 1);
        assert!(!next.revealed);
        assert!(tmp
            .collection
            .list_review_logs(&deck.id)
            .unwrap()
            .is_empty());

        sessions.end(&started.session_id).unwrap();
    }

    #[test]
    fn unknown_session_id_errors_consistently_and_end_is_idempotent() {
        let tmp = TempCollection::new("unknown");
        let sessions = StudySessions::default();

        for err in [
            sessions.reveal("study-999", NOW).unwrap_err(),
            sessions
                .grade(&tmp.collection, "study-999", Grade::Good, NOW)
                .unwrap_err(),
            sessions.advance_preview("study-999", NOW).unwrap_err(),
            sessions
                .undo_last_grade(&tmp.collection, "study-999", NOW)
                .unwrap_err(),
            sessions
                .suspend_current(&tmp.collection, "study-999", NOW)
                .unwrap_err(),
        ] {
            assert_eq!(err, "study session not found: study-999");
        }

        // Ending an unknown session ID succeeds cleanly.
        sessions.end("study-999").unwrap();
    }

    #[test]
    fn concurrent_sessions_receive_unique_ids_and_remain_isolated() {
        let tmp = TempCollection::new("concurrent");
        let deck_a = tmp.collection.create_deck("DeckA", NOW).unwrap();
        let deck_b = tmp.collection.create_deck("DeckB", NOW).unwrap();
        tmp.collection
            .create_card(&deck_a.id, "QA", "AA", vec![], NOW)
            .unwrap();
        tmp.collection
            .create_card(&deck_b.id, "QB", "AB", vec![], NOW)
            .unwrap();

        let sessions = Arc::new(StudySessions::default());
        let s1 = sessions
            .start(
                &tmp.collection,
                StudyRequest::deck(deck_a.id.clone()),
                NOW + 10,
            )
            .unwrap();
        let s2 = sessions
            .start(
                &tmp.collection,
                StudyRequest::deck(deck_b.id.clone()),
                NOW + 10,
            )
            .unwrap();
        assert_eq!(s1.session_id, "study-1");
        assert_eq!(s2.session_id, "study-2");

        // Ending session 1 does not affect session 2.
        sessions.end(&s1.session_id).unwrap();
        assert!(sessions.reveal(&s1.session_id, NOW + 20).is_err());
        let s2_revealed = sessions.reveal(&s2.session_id, NOW + 20).unwrap();
        assert!(s2_revealed.revealed);
        assert_eq!(
            s2_revealed.current.as_ref().map(|c| c.front.as_str()),
            Some("QB")
        );

        // Concurrent spawns from multiple threads generate strictly unique IDs.
        let handles: Vec<_> = (0..6)
            .map(|_| {
                let sessions = Arc::clone(&sessions);
                let db_path = tmp.dir.join("collection.sqlite3");
                let deck_id = deck_a.id.clone();
                std::thread::spawn(move || {
                    let col = Collection::open(db_path).unwrap();
                    sessions
                        .start(&col, StudyRequest::deck(deck_id), NOW + 30)
                        .unwrap()
                        .session_id
                })
            })
            .collect();

        let mut ids: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 6);
    }
}
