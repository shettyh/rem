use std::cell::Cell;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};
use rem_core::{
    has_image_occlusion, render_cloze_answer, render_cloze_question, render_occlusion_tui_summary,
    CardProvenance, Collection, CollectionError, Grade, LeechAction, StudyGradeOutcome,
    StudyRequest, StudySession, StudyView,
};

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
    card_started_at: i64,
    card_elapsed_secs: u64,
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
            card_started_at: now,
            card_elapsed_secs: 0,
            conflict_message: None,
            status_message: None,
        })
    }

    pub fn view(&self) -> &StudyView {
        &self.view
    }

    pub fn tick(&mut self, now: i64) {
        if self.view.current.is_some() && !self.view.revealed {
            self.card_elapsed_secs =
                (now.saturating_sub(self.card_started_at).max(0) / 1_000) as u64;
        }
    }

    pub fn elapsed_secs(&self) -> u64 {
        self.card_elapsed_secs
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
                self.tick(now);
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
                self.card_started_at = now;
                self.card_elapsed_secs = 0;
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
                self.card_started_at = now;
                self.card_elapsed_secs = 0;
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
                self.card_started_at = now;
                self.card_elapsed_secs = 0;
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
        let base = format!(
            "Card {}/{} · Reviewed {} · Remaining {}",
            app.view.reviewed + 1,
            total,
            app.view.reviewed,
            app.view.remaining
        );
        if app.view.show_timer {
            let secs = app.elapsed_secs();
            format!("{base} · {}:{:02}", secs / 60, secs % 60)
        } else {
            base
        }
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

    let mut lines = vec![section_label(if has_image_occlusion(&card.front) {
        "IMAGE OCCLUSION"
    } else {
        "QUESTION"
    })];
    if let Some(cloze_idx) = card.cloze_index {
        let base_front = if has_image_occlusion(&card.front) {
            render_occlusion_tui_summary(&card.front, Some(cloze_idx), app.view.revealed)
        } else {
            card.front.clone()
        };
        let prompt = if app.view.revealed {
            render_cloze_answer(&base_front, cloze_idx)
        } else {
            render_cloze_question(&base_front, cloze_idx)
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
        let base_front = if has_image_occlusion(&card.front) {
            render_occlusion_tui_summary(&card.front, None, app.view.revealed)
        } else {
            card.front.clone()
        };
        lines.extend(markdown_lines(&base_front));
        if app.view.revealed {
            lines.push(Line::default());
            lines.push(section_label("ANSWER"));
            lines.extend(markdown_lines(&card.back));
            lines.extend(provenance_lines(&card.provenance));
        }
    }
    lines
}

/// Compute rendered line count matching ratatui's `Wrap { trim: false }` greedy word-wrapping
/// (conservative upper bound so users can always scroll to the very bottom of long content).
pub(crate) fn wrapped_line_count(lines: &[Line<'_>], width: u16) -> usize {
    let w = width.max(1) as usize;
    lines
        .iter()
        .map(|line| {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            if text.is_empty() {
                return 1;
            }
            let mut rows = 1usize;
            let mut col = 0usize;
            let mut chars = text.chars().peekable();
            while let Some(&ch) = chars.peek() {
                if ch.is_whitespace() {
                    let mut ws_width = 0usize;
                    while let Some(&c) = chars.peek() {
                        if !c.is_whitespace() {
                            break;
                        }
                        chars.next();
                        ws_width += 1;
                    }
                    if col + ws_width <= w {
                        col += ws_width;
                    } else {
                        let remaining = col + ws_width;
                        rows += (remaining - 1) / w;
                        col = ((remaining - 1) % w) + 1;
                    }
                } else {
                    let mut word_width = 0usize;
                    while let Some(&c) = chars.peek() {
                        if c.is_whitespace() {
                            break;
                        }
                        chars.next();
                        // Treat wide/non-ASCII chars conservatively (up to 2 cols) so CJK/emoji
                        // never undercount wrapped rows.
                        word_width += if c.is_ascii() { 1 } else { 2 };
                    }
                    if col > 0 && col + word_width > w {
                        rows += 1;
                        col = 0;
                    }
                    if word_width <= w {
                        col += word_width;
                    } else {
                        rows += (word_width - 1) / w;
                        col = ((word_width - 1) % w) + 1;
                    }
                }
            }
            rows
        })
        .sum()
}

pub(crate) fn section_label(label: &'static str) -> Line<'static> {
    Line::from(Span::styled(
        label,
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    ))
}

pub(crate) fn provenance_lines(provenance: &CardProvenance) -> Vec<Line<'static>> {
    if provenance.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![Line::default(), section_label("PROVENANCE")];
    if let Some(rationale) = &provenance.rationale {
        lines.push(Line::from(Span::styled(
            format!("Why: {rationale}"),
            Style::default().fg(Color::DarkGray),
        )));
    }
    for source in &provenance.sources {
        let text = match &source.label {
            Some(label) => format!("Source: {label} ({})", source.locator),
            None => format!("Source: {}", source.locator),
        };
        lines.push(Line::from(Span::styled(
            text,
            Style::default().fg(Color::DarkGray),
        )));
    }
    if let Some(producer) = &provenance.proposed_by {
        lines.push(Line::from(Span::styled(
            format!("Proposed by: {producer}"),
            Style::default().fg(Color::DarkGray),
        )));
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

pub(crate) fn markdown_lines(markdown: &str) -> Vec<Line<'static>> {
    MarkdownRenderer::render(markdown)
}

struct ListState {
    next: Option<u64>,
}

struct MarkdownRenderer {
    lines: Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
    styles: Vec<Style>,
    style: Style,
    lists: Vec<ListState>,
    link: Option<String>,
    image_depth: u32,
    code_block: bool,
}

impl MarkdownRenderer {
    fn render(markdown: &str) -> Vec<Line<'static>> {
        let mut renderer = Self {
            lines: Vec::new(),
            spans: Vec::new(),
            styles: Vec::new(),
            style: Style::default(),
            lists: Vec::new(),
            link: None,
            image_depth: 0,
            code_block: false,
        };
        let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
        for event in Parser::new_ext(markdown, options) {
            renderer.event(event);
        }
        renderer.finish_line();
        while renderer
            .lines
            .last()
            .is_some_and(|line| line.spans.is_empty())
        {
            renderer.lines.pop();
        }
        renderer.lines
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => {
                if self.image_depth == 0 {
                    self.text(&text);
                }
            }
            Event::Code(code) if self.image_depth == 0 => {
                self.spans.push(Span::styled(
                    code.into_string(),
                    self.style.fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ));
            }
            Event::SoftBreak | Event::HardBreak => self.finish_line(),
            Event::Rule => {
                self.finish_line();
                self.lines.push(Line::from(Span::styled(
                    "────────────────────",
                    Style::default().fg(Color::DarkGray),
                )));
            }
            Event::TaskListMarker(checked) if self.image_depth == 0 => self
                .spans
                .push(Span::raw(if checked { "[x] " } else { "[ ] " })),
            Event::FootnoteReference(label) if self.image_depth == 0 => {
                self.spans.push(Span::raw(format!("[{}]", label)));
            }
            Event::Code(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::TaskListMarker(_)
            | Event::FootnoteReference(_) => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { .. } => {
                self.push_style(self.style.fg(Color::Cyan).add_modifier(Modifier::BOLD))
            }
            Tag::Emphasis => self.push_style(self.style.add_modifier(Modifier::ITALIC)),
            Tag::Strong => self.push_style(self.style.add_modifier(Modifier::BOLD)),
            Tag::Strikethrough => self.push_style(self.style.add_modifier(Modifier::CROSSED_OUT)),
            Tag::Link { dest_url, .. } => {
                let url = dest_url.into_string();
                if url.starts_with("asset:") {
                    self.link = Some(url);
                    self.push_style(Style::default().fg(Color::Magenta));
                    self.spans.push(Span::styled("[", self.style));
                } else {
                    self.link = Some(url);
                    self.push_style(
                        self.style
                            .fg(Color::Blue)
                            .add_modifier(Modifier::UNDERLINED),
                    );
                }
            }
            Tag::Image { dest_url, .. } => {
                if self.image_depth == 0 {
                    self.spans.push(Span::styled(
                        image_placeholder(&dest_url),
                        Style::default().fg(Color::Magenta),
                    ));
                }
                self.image_depth += 1;
            }
            Tag::List(start) => self.lists.push(ListState { next: start }),
            Tag::Item if self.image_depth == 0 => {
                self.finish_line();
                let prefix = self
                    .lists
                    .last_mut()
                    .map(|list| match list.next.as_mut() {
                        Some(next) => {
                            let prefix = format!("{next}. ");
                            *next += 1;
                            prefix
                        }
                        None => "• ".into(),
                    })
                    .unwrap_or_else(|| "• ".into());
                self.spans.push(Span::raw(prefix));
            }
            Tag::CodeBlock(_) => {
                self.finish_line();
                self.code_block = true;
                self.push_style(Style::default().fg(Color::Yellow));
            }
            Tag::BlockQuote(_) => {
                self.finish_line();
                self.spans
                    .push(Span::styled("│ ", Style::default().fg(Color::DarkGray)));
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_) => {
                self.pop_style();
                self.finish_line();
                self.blank_line();
            }
            TagEnd::Paragraph => {
                self.finish_line();
                if self.lists.is_empty() {
                    self.blank_line();
                }
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => self.pop_style(),
            TagEnd::Link => {
                if let Some(link) = self.link.take() {
                    if link.starts_with("asset:") {
                        self.spans.push(Span::styled("]", self.style));
                        self.pop_style();
                    } else {
                        self.pop_style();
                        self.spans.push(Span::styled(
                            format!(" ({link})"),
                            Style::default().fg(Color::Blue),
                        ));
                    }
                } else {
                    self.pop_style();
                }
            }
            TagEnd::Image => self.image_depth = self.image_depth.saturating_sub(1),
            TagEnd::Item => self.finish_line(),
            TagEnd::List(_) => {
                self.lists.pop();
                self.blank_line();
            }
            TagEnd::CodeBlock => {
                self.finish_line();
                self.code_block = false;
                self.pop_style();
                self.blank_line();
            }
            TagEnd::BlockQuote(_) => self.finish_line(),
            _ => {}
        }
    }

    fn text(&mut self, text: &str) {
        if self.code_block {
            for (index, line) in text.split('\n').enumerate() {
                if index > 0 {
                    self.finish_line();
                }
                if !line.is_empty() {
                    if self.spans.is_empty() {
                        self.spans.push(Span::raw("  "));
                    }
                    self.spans.push(Span::styled(line.to_owned(), self.style));
                }
            }
        } else {
            self.spans.push(Span::styled(text.to_owned(), self.style));
        }
    }

    fn push_style(&mut self, style: Style) {
        self.styles.push(self.style);
        self.style = style;
    }

    fn pop_style(&mut self) {
        self.style = self.styles.pop().unwrap_or_default();
    }

    fn finish_line(&mut self) {
        if !self.spans.is_empty() {
            self.lines.push(Line::from(std::mem::take(&mut self.spans)));
        }
    }

    fn blank_line(&mut self) {
        if self.lines.last().is_some_and(|line| !line.spans.is_empty()) {
            self.lines.push(Line::default());
        }
    }
}

fn image_placeholder(destination: &str) -> String {
    match destination.split_once("asset:") {
        Some((_prefix, hash)) => format!("[image/GIF: asset:{hash}]"),
        None => format!("[image/GIF: {destination}]"),
    }
}
