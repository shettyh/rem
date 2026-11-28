use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use rem_core::CardProvenance;

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
    in_table_cell: bool,
    table_cell_index: usize,
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
            in_table_cell: false,
            table_cell_index: 0,
        };
        let options =
            Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS | Options::ENABLE_TABLES;
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
            Event::Text(text) => {
                if self.image_depth == 0 {
                    self.text(&text);
                }
            }
            Event::Html(_) | Event::InlineHtml(_) => {}
            Event::Code(code) if self.image_depth == 0 => {
                self.spans.push(Span::styled(
                    code.into_string(),
                    self.style.fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ));
            }
            Event::SoftBreak | Event::HardBreak => {
                if self.in_table_cell {
                    self.spans.push(Span::raw(" "));
                } else {
                    self.finish_line();
                }
            }
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
            Tag::Table(_) => {
                self.finish_line();
            }
            Tag::TableHead => {
                self.table_cell_index = 0;
                self.push_style(self.style.add_modifier(Modifier::BOLD));
            }
            Tag::TableRow => {
                self.table_cell_index = 0;
            }
            Tag::TableCell => {
                if self.table_cell_index > 0 {
                    self.spans
                        .push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
                }
                self.table_cell_index += 1;
                self.in_table_cell = true;
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
            TagEnd::TableCell => {
                self.in_table_cell = false;
            }
            TagEnd::TableHead => {
                self.pop_style();
                self.finish_line();
                self.lines.push(Line::from(Span::styled(
                    "────────────────────",
                    Style::default().fg(Color::DarkGray),
                )));
            }
            TagEnd::TableRow => {
                self.finish_line();
            }
            TagEnd::Table => {
                self.blank_line();
            }
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
            self.push_text_with_ruby(text);
        }
    }

    fn push_text_with_ruby(&mut self, text: &str) {
        if !text.contains('{') {
            self.spans.push(Span::styled(text.to_owned(), self.style));
            return;
        }
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            if open > 0 {
                self.spans
                    .push(Span::styled(rest[..open].to_owned(), self.style));
            }
            let after_open = &rest[open + 1..];
            if let Some(close) = after_open.find('}') {
                let inner = &after_open[..close];
                if let Some((base, reading)) = inner.split_once('|') {
                    let b = base.trim();
                    let r = reading.trim();
                    if !b.is_empty()
                        && !r.is_empty()
                        && !b.contains(['{', '}'])
                        && !r.contains(['{', '}', '|'])
                    {
                        self.spans.push(Span::styled(b.to_owned(), self.style));
                        self.spans.push(Span::styled(
                            format!("({r})"),
                            Style::default().fg(Color::DarkGray),
                        ));
                        rest = &after_open[close + 1..];
                        continue;
                    }
                }
            }
            self.spans.push(Span::styled("{".to_owned(), self.style));
            rest = after_open;
        }
        if !rest.is_empty() {
            self.spans.push(Span::styled(rest.to_owned(), self.style));
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
