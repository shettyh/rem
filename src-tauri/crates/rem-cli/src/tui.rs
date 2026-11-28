//! Interactive terminal study adapter (`rem study`).
//!
//! # Frozen scope
//!
//! `rem study` is intentionally frozen to a minimal, low-maintenance review surface over the
//! shared `rem_core::StudySession`:
//! - All-deck or single-deck (`--deck`) due review
//! - Reveal (`Space`/`Enter`), FSRS-7 grading (`1`–`4`) with interval previews, undo (`u`/`Ctrl+Z`),
//!   suspend (`s`), scroll (`↑`/`↓`/`j`/`k`), and clean quit (`q`/`Ctrl+C`)
//! - Basic Markdown, basic cloze (`{{cN::...}}`), provenance, and graceful placeholders for media
//!   and Image Occlusion cards
//!
//! Desktop and iOS are the primary full-fidelity study surfaces. Custom study modes, answer
//! timers, inline media/audio playback, interactive Image Occlusion, LaTeX typesetting, and
//! mid-session card editing are intentionally out of scope for `rem study`.

use std::cell::Cell;

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};
use rem_core::{
    has_image_occlusion, render_cloze_answer, render_cloze_question, strip_occlusion_markup,
    Collection, CollectionError, Grade, LeechAction, StudyGradeOutcome, StudyRequest, StudySession,
    StudyView,
};

use crate::tui_render::{markdown_lines, provenance_lines, section_label, wrapped_line_count};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StudyScope {
    AllDecks,
    Deck(String),
}

impl StudyScope {
    fn label(&self) -> &str {
        match self {
            Self::AllDecks => "All decks",
            Self::Deck(name) => name,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudyInput {
    Reveal,
    Grade(Grade),
    Undo,
    Suspend,
    ScrollUp,
    ScrollDown,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudyControl {
    Continue,
    Quit,
}

#[derive(Debug)]
pub struct StudyApp {
    session: StudySession,
    scope: StudyScope,
    view: StudyView,
    scroll: u16,
    max_scroll: Cell<u16>,
    interval_base: Option<i64>,
    conflict_message: Option<String>,
    status_message: Option<String>,
}

impl StudyApp {
    pub fn start(
        collection: &Collection,
        request: StudyRequest,
        scope: StudyScope,
        now: i64,
    ) -> Result<Self, CollectionError> {
        let session = StudySession::start(collection, request, now)?;
        let view = session.view();
        Ok(Self {
            session,
            scope,
            view,
            scroll: 0,
            max_scroll: Cell::new(u16::MAX),
            interval_base: None,
            conflict_message: None,
            status_message: None,
        })
    }

    pub fn view(&self) -> &StudyView {
        &self.view
    }

    pub fn handle(
        &mut self,
        collection: &Collection,
        input: StudyInput,
        now: i64,
    ) -> Result<StudyControl, CollectionError> {
        match input {
            StudyInput::Quit => return Ok(StudyControl::Quit),
            StudyInput::ScrollUp => self.scroll = self.scroll.saturating_sub(1),
            StudyInput::ScrollDown => {
                let limit = self.max_scroll.get();
                self.scroll = self.scroll.saturating_add(1).min(limit);
            }
            StudyInput::Reveal if self.view.current.is_some() && !self.view.revealed => {
                self.view = self.session.reveal(now)?;
                self.interval_base = Some(now);
                self.status_message = None;
                self.scroll = 0;
                self.max_scroll.set(u16::MAX);
            }
            StudyInput::Grade(grade)
                if self.view.current.is_some()
                    && self.view.revealed
                    && self.view.next_states.is_some() =>
            {
                match self.session.grade(collection, grade, now)? {
                    StudyGradeOutcome::Graded { view } => {
                        self.view = view;
                        self.conflict_message = None;
                        self.status_message = None;
                    }
                    StudyGradeOutcome::Conflict { view, .. } => {
                        self.view = view;
                        self.conflict_message = Some(
                            "Card changed; stale review skipped without recording a review.".into(),
                        );
                        self.status_message = None;
                    }
                }
                self.interval_base = None;
                self.scroll = 0;
                self.max_scroll.set(u16::MAX);
            }
            StudyInput::Undo if self.view.can_undo => {
                match self.session.undo_last_grade(collection, now) {
                    Ok(view) => {
                        self.view = view;
                        self.conflict_message = None;
                        self.status_message = Some("Last grade undone.".into());
                    }
                    Err(CollectionError::Conflict { .. } | CollectionError::NotFound { .. }) => {
                        self.view = self.session.view();
                        self.conflict_message = None;
                        self.status_message = Some(
                            "Cannot undo: previous card was changed or deleted elsewhere.".into(),
                        );
                    }
                    Err(other) => return Err(other),
                }
                self.interval_base = None;
                self.scroll = 0;
                self.max_scroll.set(u16::MAX);
            }
            StudyInput::Suspend if self.view.current.is_some() => {
                match self.session.suspend_current(collection, now) {
                    Ok(view) => {
                        self.view = view;
                        self.conflict_message = None;
                        self.status_message = Some("Card suspended.".into());
                    }
                    Err(CollectionError::NotFound { .. }) => {
                        self.view = self.session.reload_current(collection, now)?;
                        self.conflict_message = None;
                        self.status_message = Some("Card was already deleted elsewhere.".into());
                    }
                    Err(other) => return Err(other),
                }
                self.interval_base = None;
                self.scroll = 0;
                self.max_scroll.set(u16::MAX);
            }
            StudyInput::Reveal | StudyInput::Grade(_) | StudyInput::Undo | StudyInput::Suspend => {}
        }
        Ok(StudyControl::Continue)
    }

    pub fn scope_label(&self) -> &str {
        self.scope.label()
    }

    pub fn conflict_message(&self) -> Option<&str> {
        self.conflict_message.as_deref()
    }

    pub fn status_message(&self) -> Option<&str> {
        self.status_message.as_deref()
    }
}

pub fn draw(frame: &mut Frame<'_>, app: &StudyApp) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let total = app.view.reviewed + app.view.remaining;
    let progress = if app.view.current.is_some() {
        format!(
            "Card {}/{} · Reviewed {} · Remaining {}",
            app.view.reviewed + 1,
            total,
            app.view.reviewed,
            app.view.remaining
        )
    } else {
        format!(
            "Reviewed {} · Remaining {}",
            app.view.reviewed, app.view.remaining
        )
    };
    let notice = app
        .conflict_message()
        .or_else(|| app.status_message())
        .map(str::to_owned)
        .or_else(|| match app.view.notice {
            Some(LeechAction::Tag) => Some("Leech tagged.".into()),
            Some(LeechAction::Suspend) => Some("Leech suspended.".into()),
            None => None,
        })
        .unwrap_or_default();
    let rule = "─".repeat(frame.area().width as usize);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("rem study", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" · "),
                Span::styled(
                    app.scope_label().to_owned(),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(progress),
            Line::from(Span::styled(notice, Style::default().fg(Color::Yellow))),
            Line::from(Span::styled(
                rule.clone(),
                Style::default().fg(Color::DarkGray),
            )),
        ]),
        areas[0],
    );

    let lines = content_lines(app);
    let max_scroll = wrapped_line_count(&lines, areas[1].width)
        .saturating_sub(areas[1].height as usize)
        .min(u16::MAX as usize) as u16;
    app.max_scroll.set(max_scroll);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((app.scroll.min(max_scroll), 0)),
        areas[1],
    );

    let secondary = if app.view.can_undo {
        "u undo · s suspend · ↑/↓/j/k scroll · q end session"
    } else {
        "s suspend · ↑/↓/j/k scroll · q end session"
    };
    let footer = if app.view.current.is_none() {
        let hint = if app.view.can_undo {
            "u undo last grade · q exit"
        } else {
            "q exit"
        };
        vec![
            Line::from(Span::styled(rule, Style::default().fg(Color::DarkGray))),
            Line::from(hint),
            Line::default(),
        ]
    } else if app.view.revealed {
        vec![
            Line::from(Span::styled(rule, Style::default().fg(Color::DarkGray))),
            grade_choices(app),
            Line::from(secondary),
        ]
    } else {
        vec![
            Line::from(Span::styled(rule, Style::default().fg(Color::DarkGray))),
            Line::from("Space/Enter reveal"),
            Line::from(secondary),
        ]
    };
    frame.render_widget(Paragraph::new(footer), areas[2]);
}

fn content_lines(app: &StudyApp) -> Vec<Line<'static>> {
    let Some(card) = &app.view.current else {
        let title = if app.conflict_message().is_some() {
            "Card changed"
        } else if app.view.reviewed == 0 {
            "Nothing due"
        } else {
            "Review complete"
        };
        return vec![
            Line::from(Span::styled(
                title,
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::default(),
            Line::from(format!("Reviewed: {}", app.view.reviewed)),
            Line::from(format!("Remaining: {}", app.view.remaining)),
        ];
    };

    if has_image_occlusion(&card.front) {
        let mut lines = vec![
            section_label("IMAGE OCCLUSION"),
            Line::from(Span::styled(
                "Visual Image Occlusion card — open in the rem desktop or iPhone app to study visually.",
                Style::default().fg(Color::DarkGray),
            )),
            Line::default(),
        ];
        lines.extend(markdown_lines(&strip_occlusion_markup(&card.front)));
        if app.view.revealed {
            if !card.back.trim().is_empty() {
                lines.push(Line::default());
                lines.push(section_label(if card.cloze_index.is_some() {
                    "EXTRA"
                } else {
                    "ANSWER"
                }));
                lines.extend(markdown_lines(&card.back));
            }
            lines.extend(provenance_lines(&card.provenance));
        }
        return lines;
    }

    let mut lines = vec![section_label("QUESTION")];
    if let Some(cloze_idx) = card.cloze_index {
        let prompt = if app.view.revealed {
            render_cloze_answer(&card.front, cloze_idx)
        } else {
            render_cloze_question(&card.front, cloze_idx)
        };
        lines.extend(markdown_lines(&prompt));
        if app.view.revealed {
            if !card.back.trim().is_empty() {
                lines.push(Line::default());
                lines.push(section_label("EXTRA"));
                lines.extend(markdown_lines(&card.back));
            }
            lines.extend(provenance_lines(&card.provenance));
        }
    } else {
        lines.extend(markdown_lines(&card.front));
        if app.view.revealed {
            lines.push(Line::default());
            lines.push(section_label("ANSWER"));
            lines.extend(markdown_lines(&card.back));
            lines.extend(provenance_lines(&card.provenance));
        }
    }
    lines
}

fn grade_choices(app: &StudyApp) -> Line<'static> {
    let Some(next) = &app.view.next_states else {
        return Line::from("Scheduling choices unavailable");
    };
    let base = app.interval_base.unwrap_or(0);
    Line::from(vec![
        Span::styled(
            format!("1 Again {}", format_interval(next.again.due - base)),
            Style::default().fg(Color::Red),
        ),
        Span::raw(" · "),
        Span::styled(
            format!("2 Hard {}", format_interval(next.hard.due - base)),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw(" · "),
        Span::styled(
            format!("3 Good {}", format_interval(next.good.due - base)),
            Style::default().fg(Color::Green),
        ),
        Span::raw(" · "),
        Span::styled(
            format!("4 Easy {}", format_interval(next.easy.due - base)),
            Style::default().fg(Color::Cyan),
        ),
    ])
}

fn format_interval(milliseconds: i64) -> String {
    let minutes = ((milliseconds as f64 / 60_000.0).round() as i64).max(1);
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = (milliseconds as f64 / 3_600_000.0).round() as i64;
    if hours < 24 {
        return format!("{hours}h");
    }
    let days = (milliseconds as f64 / 86_400_000.0).round() as i64;
    if days < 30 {
        return format!("{days}d");
    }
    if days < 365 {
        return format!("{}mo", (days as f64 / 30.0).round() as i64);
    }
    format!("{}y", (days as f64 / 365.0).round() as i64)
}
