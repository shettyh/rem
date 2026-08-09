use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};
use rem_core::{
    Collection, CollectionError, Grade, LeechAction, StudyGradeOutcome, StudyRequest, StudySession,
    StudyView,
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
    interval_base: Option<i64>,
    conflict_message: Option<String>,
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
            interval_base: None,
            conflict_message: None,
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
            StudyInput::ScrollDown => self.scroll = self.scroll.saturating_add(1),
            StudyInput::Reveal if self.view.current.is_some() && !self.view.revealed => {
                self.view = self.session.reveal(now)?;
                self.interval_base = Some(now);
                self.scroll = 0;
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
                    }
                    StudyGradeOutcome::Conflict { view, .. } => {
                        self.view = view;
                        self.conflict_message = Some(
                            "Card changed; stale review skipped without recording a review.".into(),
                        );
                    }
                }
                self.interval_base = None;
                self.scroll = 0;
            }
            StudyInput::Reveal | StudyInput::Grade(_) => {}
        }
        Ok(StudyControl::Continue)
    }

    pub fn scope_label(&self) -> &str {
        self.scope.label()
    }

    pub fn conflict_message(&self) -> Option<&str> {
        self.conflict_message.as_deref()
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
        .split(frame.size());

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
        .map(str::to_owned)
        .or_else(|| match app.view.notice {
            Some(LeechAction::Tag) => Some("Leech tagged.".into()),
            Some(LeechAction::Suspend) => Some("Leech suspended.".into()),
            None => None,
        })
        .unwrap_or_default();
    let rule = "─".repeat(frame.size().width as usize);
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

    frame.render_widget(
        Paragraph::new(content_lines(app))
            .scroll((app.scroll, 0))
            .wrap(Wrap { trim: false }),
        areas[1],
    );

    let footer = if app.view.current.is_none() {
        vec![
            Line::from(Span::styled(rule, Style::default().fg(Color::DarkGray))),
            Line::from("q Exit"),
            Line::default(),
        ]
    } else if app.view.revealed {
        vec![
            Line::from(Span::styled(rule, Style::default().fg(Color::DarkGray))),
            grade_choices(app),
            Line::from("↑/↓/j/k scroll · q end session"),
        ]
    } else {
        vec![
            Line::from(Span::styled(rule, Style::default().fg(Color::DarkGray))),
            Line::from("Space/Enter reveal"),
            Line::from("↑/↓/j/k scroll · q end session"),
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

    let mut lines = vec![section_label("QUESTION")];
    lines.extend(markdown_lines(&card.front));
    if app.view.revealed {
        lines.push(Line::default());
        lines.push(section_label("ANSWER"));
        lines.extend(markdown_lines(&card.back));
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

fn markdown_lines(markdown: &str) -> Vec<Line<'static>> {
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
            Event::Code(_) | Event::TaskListMarker(_) | Event::FootnoteReference(_) => {}
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
                self.link = Some(dest_url.into_string());
                self.push_style(
                    self.style
                        .fg(Color::Blue)
                        .add_modifier(Modifier::UNDERLINED),
                );
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
            Tag::BlockQuote => {
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
                self.pop_style();
                if let Some(link) = self.link.take() {
                    self.spans.push(Span::styled(
                        format!(" ({link})"),
                        Style::default().fg(Color::Blue),
                    ));
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
            TagEnd::BlockQuote => self.finish_line(),
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
