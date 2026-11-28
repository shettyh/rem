use std::cell::Cell;
use std::collections::HashMap;

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};
use rem_core::{
    CardDraft, Collection, CollectionError, DraftDecision, DraftResolution, NewCardInput,
};

use crate::editor::{format_draft_for_editor, parse_draft_from_editor};
use crate::tui_render::{markdown_lines, provenance_lines, section_label, wrapped_line_count};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftEditState {
    pub deck_id: String,
    pub front: String,
    pub back: String,
    pub tags: Vec<String>,
}

impl DraftEditState {
    fn from_draft(draft: &CardDraft) -> Self {
        Self {
            deck_id: draft.deck_id.clone(),
            front: draft.front.clone(),
            back: draft.back.clone(),
            tags: draft.tags.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftInput {
    Reveal,
    Accept,
    Reject,
    Edit,
    Skip,
    ScrollUp,
    ScrollDown,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftControl {
    Continue,
    EditInEditor,
    Quit,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DraftTriageSummary {
    pub accepted: usize,
    pub existing: usize,
    pub rejected: usize,
    pub remaining: usize,
}

#[derive(Debug)]
pub struct DraftApp {
    deck_names: HashMap<String, String>,
    scope_label: String,
    drafts: Vec<CardDraft>,
    edits: HashMap<String, DraftEditState>,
    edit: Option<DraftEditState>,
    revealed: bool,
    scroll: u16,
    max_scroll: Cell<u16>,
    accepted: usize,
    existing: usize,
    rejected: usize,
    status_message: Option<String>,
}

impl DraftApp {
    pub fn start(
        collection: &Collection,
        deck_filter: Option<(&str, &str)>,
    ) -> Result<Self, CollectionError> {
        let decks = collection.list_decks()?;
        let deck_names: HashMap<String, String> =
            decks.into_iter().map(|deck| (deck.id, deck.name)).collect();
        let all_drafts = collection.list_drafts()?;
        let (drafts, scope_label) = match deck_filter {
            Some((deck_id, deck_name)) => (
                all_drafts
                    .into_iter()
                    .filter(|draft| draft.deck_id == deck_id)
                    .collect(),
                deck_name.to_owned(),
            ),
            None => (all_drafts, "All decks".to_owned()),
        };
        let edit = drafts.first().map(DraftEditState::from_draft);
        Ok(Self {
            deck_names,
            scope_label,
            drafts,
            edits: HashMap::new(),
            edit,
            revealed: false,
            scroll: 0,
            max_scroll: Cell::new(u16::MAX),
            accepted: 0,
            existing: 0,
            rejected: 0,
            status_message: None,
        })
    }

    pub fn current(&self) -> Option<&CardDraft> {
        self.drafts.first()
    }

    pub fn current_edit(&self) -> Option<&DraftEditState> {
        self.edit.as_ref()
    }

    pub fn revealed(&self) -> bool {
        self.revealed
    }

    pub fn pending(&self) -> usize {
        self.drafts.len()
    }

    pub fn status_message(&self) -> Option<&str> {
        self.status_message.as_deref()
    }

    pub fn set_status_message(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
    }

    pub fn summary(&self) -> DraftTriageSummary {
        DraftTriageSummary {
            accepted: self.accepted,
            existing: self.existing,
            rejected: self.rejected,
            remaining: self.drafts.len(),
        }
    }

    pub fn current_deck_name(&self) -> &str {
        self.edit
            .as_ref()
            .and_then(|edit| self.deck_names.get(&edit.deck_id))
            .or_else(|| {
                self.current()
                    .and_then(|draft| self.deck_names.get(&draft.deck_id))
            })
            .map(String::as_str)
            .unwrap_or("Deck")
    }

    pub fn format_current_for_editor(&self) -> Option<String> {
        let edit = self.edit.as_ref()?;
        let deck_label = match self.deck_names.get(&edit.deck_id) {
            Some(name)
                if self
                    .deck_names
                    .values()
                    .filter(|candidate| candidate.as_str() == name.as_str())
                    .count()
                    == 1 =>
            {
                name.as_str()
            }
            _ => edit.deck_id.as_str(),
        };
        Some(format_draft_for_editor(
            deck_label,
            &edit.tags,
            &edit.front,
            &edit.back,
        ))
    }

    pub fn apply_editor_output(&mut self, text: &str) {
        let Some(current) = self.edit.as_ref() else {
            return;
        };
        let current_deck_id = current.deck_id.clone();
        match parse_draft_from_editor(text) {
            Ok(Some(parsed)) => {
                let resolved_deck_id = match parsed.deck {
                    Some(reference) => match self.resolve_deck_reference(&reference) {
                        Ok(deck_id) => deck_id,
                        Err(message) => {
                            self.status_message = Some(message);
                            return;
                        }
                    },
                    None => current_deck_id,
                };
                let next_edit = DraftEditState {
                    deck_id: resolved_deck_id,
                    front: parsed.front,
                    back: parsed.back,
                    tags: parsed.tags,
                };
                if let Some(draft) = self.drafts.first() {
                    self.edits.insert(draft.id.clone(), next_edit.clone());
                }
                self.edit = Some(next_edit);
                self.revealed = true;
                self.scroll = 0;
                self.status_message = Some("Draft updated in memory. Press a to accept.".into());
            }
            Ok(None) => {
                self.status_message = Some("Edit cancelled (front left blank).".into());
            }
            Err(message) => {
                self.status_message = Some(format!("Could not parse edited draft: {message}"));
            }
        }
    }

    fn resolve_deck_reference(&self, reference: &str) -> Result<String, String> {
        if self.deck_names.contains_key(reference) {
            return Ok(reference.to_owned());
        }
        let matches: Vec<&String> = self
            .deck_names
            .iter()
            .filter_map(|(id, name)| (name == reference).then_some(id))
            .collect();
        match matches.as_slice() {
            [id] => Ok((*id).clone()),
            [] => Err(format!("Unknown deck in edited draft: `{reference}`.")),
            _ => Err(format!(
                "Deck name `{reference}` is ambiguous; use the deck ID instead."
            )),
        }
    }

    pub fn handle(
        &mut self,
        collection: &Collection,
        input: DraftInput,
        now: i64,
    ) -> Result<DraftControl, CollectionError> {
        match input {
            DraftInput::Quit => return Ok(DraftControl::Quit),
            DraftInput::ScrollUp => self.scroll = self.scroll.saturating_sub(1),
            DraftInput::ScrollDown => {
                let limit = self.max_scroll.get();
                self.scroll = self.scroll.saturating_add(1).min(limit);
            }
            DraftInput::Reveal if self.current().is_some() && !self.revealed => {
                self.revealed = true;
                self.status_message = None;
                self.scroll = 0;
            }
            DraftInput::Edit if self.current().is_some() => {
                return Ok(DraftControl::EditInEditor);
            }
            DraftInput::Skip if self.drafts.len() > 1 => {
                let skipped = self.drafts.remove(0);
                if let Some(edit) = self.edit.take() {
                    self.edits.insert(skipped.id.clone(), edit);
                }
                self.drafts.push(skipped);
                self.load_head();
                self.status_message = Some("Skipped draft for now.".into());
            }
            DraftInput::Accept if self.revealed && self.current().is_some() => {
                let draft = self.drafts[0].clone();
                let edit = self
                    .edit
                    .clone()
                    .unwrap_or_else(|| DraftEditState::from_draft(&draft));
                let decision = DraftDecision::Accept {
                    deck_id: edit.deck_id,
                    card: NewCardInput {
                        front: edit.front,
                        back: edit.back,
                        tags: edit.tags,
                    },
                };
                match collection.resolve_draft(&draft.id, draft.revision, decision, now) {
                    Ok(DraftResolution::Accepted(_)) => {
                        self.accepted += 1;
                        self.remove_head();
                        self.status_message =
                            Some("Accepted into due cards with provenance.".into());
                    }
                    Ok(DraftResolution::ExistingCard(_)) => {
                        self.existing += 1;
                        self.remove_head();
                        self.status_message =
                            Some("Matching card already exists; cleared duplicate draft.".into());
                    }
                    Ok(DraftResolution::Rejected) => {
                        self.remove_head();
                    }
                    Err(CollectionError::NotFound { kind: "deck", .. }) => {
                        self.status_message = Some(
                            "Target deck no longer exists; press e to pick another deck.".into(),
                        );
                    }
                    Err(CollectionError::Conflict { .. } | CollectionError::NotFound { .. }) => {
                        self.remove_head();
                        self.status_message = Some(
                            "Draft changed or was already resolved elsewhere; skipped.".into(),
                        );
                    }
                    Err(CollectionError::InvalidInput(message)) => {
                        self.status_message = Some(format!("Cannot accept: {message}"));
                    }
                    Err(other) => return Err(other),
                }
            }
            DraftInput::Reject if self.current().is_some() => {
                let draft = self.drafts[0].clone();
                match collection.resolve_draft(
                    &draft.id,
                    draft.revision,
                    DraftDecision::Reject,
                    now,
                ) {
                    Ok(_) => {
                        self.rejected += 1;
                        self.remove_head();
                        self.status_message = Some("Draft rejected.".into());
                    }
                    Err(CollectionError::Conflict { .. } | CollectionError::NotFound { .. }) => {
                        self.remove_head();
                        self.status_message = Some(
                            "Draft changed or was already resolved elsewhere; skipped.".into(),
                        );
                    }
                    Err(other) => return Err(other),
                }
            }
            DraftInput::Reveal
            | DraftInput::Accept
            | DraftInput::Reject
            | DraftInput::Edit
            | DraftInput::Skip => {}
        }
        Ok(DraftControl::Continue)
    }

    fn remove_head(&mut self) {
        if !self.drafts.is_empty() {
            let removed = self.drafts.remove(0);
            self.edits.remove(&removed.id);
        }
        self.load_head();
    }

    fn load_head(&mut self) {
        self.edit = self.drafts.first().map(|draft| {
            self.edits
                .get(&draft.id)
                .cloned()
                .unwrap_or_else(|| DraftEditState::from_draft(draft))
        });
        self.revealed = false;
        self.scroll = 0;
        self.max_scroll.set(u16::MAX);
    }
}

pub fn draw_draft(frame: &mut Frame<'_>, app: &DraftApp) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let decided = app.accepted + app.existing + app.rejected;
    let total = decided + app.pending();
    let progress = if app.current().is_some() {
        format!(
            "Draft {}/{} · {} · Accepted {} · Rejected {}",
            decided + 1,
            total,
            app.current_deck_name(),
            app.accepted,
            app.rejected,
        )
    } else {
        format!(
            "Inbox clear · Accepted {} · Rejected {}",
            app.accepted, app.rejected,
        )
    };
    let notice = app.status_message().unwrap_or_default().to_owned();
    let rule = "─".repeat(frame.area().width as usize);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    "rem draft review",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(" · "),
                Span::styled(app.scope_label.clone(), Style::default().fg(Color::Cyan)),
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

    let lines = draft_content_lines(app);
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

    let footer = if app.current().is_none() {
        vec![
            Line::from(Span::styled(rule, Style::default().fg(Color::DarkGray))),
            Line::from("q exit"),
            Line::default(),
        ]
    } else if app.revealed() {
        let primary = if app.pending() > 1 {
            "a accept · r reject · e edit in $EDITOR · n skip"
        } else {
            "a accept · r reject · e edit in $EDITOR"
        };
        vec![
            Line::from(Span::styled(rule, Style::default().fg(Color::DarkGray))),
            Line::from(Span::styled(primary, Style::default().fg(Color::Green))),
            Line::from("↑/↓/j/k scroll · q exit"),
        ]
    } else {
        let secondary = if app.pending() > 1 {
            "e edit · r reject · n skip · ↑/↓/j/k scroll · q exit"
        } else {
            "e edit · r reject · ↑/↓/j/k scroll · q exit"
        };
        vec![
            Line::from(Span::styled(rule, Style::default().fg(Color::DarkGray))),
            Line::from("Space/Enter reveal proposal"),
            Line::from(secondary),
        ]
    };
    frame.render_widget(Paragraph::new(footer), areas[2]);
}

fn draft_content_lines(app: &DraftApp) -> Vec<Line<'static>> {
    let (Some(draft), Some(edit)) = (app.current(), app.current_edit()) else {
        let mut lines = vec![
            Line::from(Span::styled(
                "Inbox clear",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::default(),
            Line::from(format!("Accepted: {}", app.accepted)),
            Line::from(format!("Rejected: {}", app.rejected)),
        ];
        if app.existing > 0 {
            lines.push(Line::from(format!(
                "Already existed (cleared): {}",
                app.existing
            )));
        }
        return lines;
    };

    let mut lines = Vec::new();
    if !edit.tags.is_empty() {
        lines.push(Line::from(Span::styled(
            edit.tags
                .iter()
                .map(|tag| format!("#{tag}"))
                .collect::<Vec<_>>()
                .join("  "),
            Style::default().fg(Color::Cyan),
        )));
        lines.push(Line::default());
    }

    lines.push(section_label("QUESTION (TRY BEFORE REVEAL)"));
    lines.extend(markdown_lines(&edit.front));

    if app.revealed() {
        lines.push(Line::default());
        lines.push(section_label("PROPOSED ANSWER"));
        if edit.back.trim().is_empty() {
            lines.push(Line::from(Span::styled(
                "(empty)",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            lines.extend(markdown_lines(&edit.back));
        }
        lines.extend(provenance_lines(&draft.provenance()));
    }

    lines
}
