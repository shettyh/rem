use std::collections::HashMap;

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};
use rem_core::{
    normalize_user_tags, CardDraft, Collection, CollectionError, DraftDecision, DraftResolution,
    NewCardInput,
};

use crate::tui::markdown_lines;

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
    edit: Option<DraftEditState>,
    revealed: bool,
    scroll: u16,
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
            edit,
            revealed: false,
            scroll: 0,
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

    pub fn summary(&self) -> DraftTriageSummary {
        DraftTriageSummary {
            accepted: self.accepted,
            existing: self.existing,
            rejected: self.rejected,
            remaining: self.drafts.len(),
        }
    }

    pub fn current_deck_name(&self) -> &str {
        self.current()
            .and_then(|draft| self.deck_names.get(&draft.deck_id))
            .map(String::as_str)
            .unwrap_or("Deck")
    }

    pub fn format_current_for_editor(&self) -> Option<String> {
        self.edit.as_ref().map(format_draft_for_editor)
    }

    pub fn apply_editor_output(&mut self, text: &str) {
        let Some(current) = self.edit.as_ref() else {
            return;
        };
        match parse_draft_from_editor(text, &current.deck_id) {
            Ok(Some(updated)) => {
                self.edit = Some(updated);
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

    pub fn handle(
        &mut self,
        collection: &Collection,
        input: DraftInput,
        now: i64,
    ) -> Result<DraftControl, CollectionError> {
        match input {
            DraftInput::Quit => return Ok(DraftControl::Quit),
            DraftInput::ScrollUp => self.scroll = self.scroll.saturating_sub(1),
            DraftInput::ScrollDown => self.scroll = self.scroll.saturating_add(1),
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
                        self.drafts.remove(0);
                        self.load_head();
                        self.status_message =
                            Some("Accepted into due cards with provenance.".into());
                    }
                    Ok(DraftResolution::ExistingCard(_)) => {
                        self.existing += 1;
                        self.drafts.remove(0);
                        self.load_head();
                        self.status_message =
                            Some("Matching card already exists; cleared duplicate draft.".into());
                    }
                    Ok(DraftResolution::Rejected) => {
                        self.drafts.remove(0);
                        self.load_head();
                    }
                    Err(CollectionError::Conflict { .. } | CollectionError::NotFound { .. }) => {
                        self.drafts.remove(0);
                        self.load_head();
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
                        self.drafts.remove(0);
                        self.load_head();
                        self.status_message = Some("Draft rejected.".into());
                    }
                    Err(CollectionError::Conflict { .. } | CollectionError::NotFound { .. }) => {
                        self.drafts.remove(0);
                        self.load_head();
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

    fn load_head(&mut self) {
        self.edit = self.drafts.first().map(DraftEditState::from_draft);
        self.revealed = false;
        self.scroll = 0;
    }
}

pub fn format_draft_for_editor(edit: &DraftEditState) -> String {
    let tags_line = edit.tags.join(", ");
    format!(
        "# Edit this draft, then save and close your editor.\n\
         # Clear the FRONT section to cancel editing.\n\
         tags: {tags_line}\n\n\
         --- FRONT ---\n\
         {}\n\n\
         --- BACK ---\n\
         {}\n",
        edit.front.trim_end_matches('\n'),
        edit.back.trim_end_matches('\n'),
    )
}

pub fn parse_draft_from_editor(
    text: &str,
    deck_id: &str,
) -> Result<Option<DraftEditState>, String> {
    let mut tags_raw = String::new();
    let mut front_lines = Vec::new();
    let mut back_lines = Vec::new();
    enum Section {
        Header,
        Front,
        Back,
    }
    let mut section = Section::Header;
    let mut saw_front = false;

    for raw_line in text.lines() {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.trim() == "--- FRONT ---" {
            section = Section::Front;
            saw_front = true;
            continue;
        }
        if line.trim() == "--- BACK ---" {
            section = Section::Back;
            continue;
        }
        match section {
            Section::Header => {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                if let Some(rest) = trimmed.strip_prefix("tags:") {
                    tags_raw = rest.trim().to_owned();
                }
            }
            Section::Front => front_lines.push(line),
            Section::Back => back_lines.push(line),
        }
    }

    if !saw_front {
        return Err("missing `--- FRONT ---` marker".into());
    }

    let front = front_lines.join("\n").trim().to_owned();
    if front.is_empty() {
        return Ok(None);
    }
    let back = back_lines.join("\n").trim().to_owned();
    let tags = normalize_user_tags(
        tags_raw
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(str::to_owned),
    );
    Ok(Some(DraftEditState {
        deck_id: deck_id.to_owned(),
        front,
        back,
        tags,
    }))
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

    let progress = if app.current().is_some() {
        format!(
            "Pending {} · Deck: {} · Accepted {} · Rejected {}",
            app.pending(),
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

    frame.render_widget(
        Paragraph::new(draft_content_lines(app))
            .scroll((app.scroll, 0))
            .wrap(Wrap { trim: false }),
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

    let mut lines = vec![section_label("QUESTION (TRY BEFORE REVEAL)")];
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

        if !edit.tags.is_empty() {
            lines.push(Line::default());
            lines.push(section_label("TAGS"));
            lines.push(Line::from(
                edit.tags
                    .iter()
                    .map(|tag| format!("#{tag}"))
                    .collect::<Vec<_>>()
                    .join("  "),
            ));
        }

        if draft.rationale.is_some() || !draft.sources.is_empty() || draft.proposed_by.is_some() {
            lines.push(Line::default());
            lines.push(section_label("PROVENANCE"));
            if let Some(rationale) = &draft.rationale {
                lines.push(Line::from(Span::styled(
                    format!("Why: {rationale}"),
                    Style::default().fg(Color::DarkGray),
                )));
            }
            for source in &draft.sources {
                let text = match &source.label {
                    Some(label) => format!("Source: {label} ({})", source.locator),
                    None => format!("Source: {}", source.locator),
                };
                lines.push(Line::from(Span::styled(
                    text,
                    Style::default().fg(Color::DarkGray),
                )));
            }
            if let Some(producer) = &draft.proposed_by {
                lines.push(Line::from(Span::styled(
                    format!("Proposed by: {producer}"),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
    }

    lines
}

fn section_label(label: &'static str) -> Line<'static> {
    Line::from(Span::styled(
        label,
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    ))
}
