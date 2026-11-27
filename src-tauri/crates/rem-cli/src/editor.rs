use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;

use rem_core::normalize_user_tags;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDraftEdit {
    pub deck: Option<String>,
    pub front: String,
    pub back: String,
    pub tags: Vec<String>,
}

pub fn format_draft_for_editor(
    deck_name: &str,
    tags: &[String],
    front: &str,
    back: &str,
) -> String {
    let tags_line = tags.join(", ");
    format!(
        "# Edit this draft, then save and close your editor.\n\
         # Change `deck:` to another deck's exact name or ID to move the draft.\n\
         # Clear the FRONT section to cancel editing.\n\
         deck: {deck_name}\n\
         tags: {tags_line}\n\n\
         --- FRONT ---\n\
         {}\n\n\
         --- BACK ---\n\
         {}\n",
        front.trim_end_matches('\n'),
        back.trim_end_matches('\n'),
    )
}

pub fn parse_draft_from_editor(text: &str) -> Result<Option<ParsedDraftEdit>, String> {
    let mut deck_raw: Option<String> = None;
    let mut tags_raw = String::new();
    let mut front_lines = Vec::new();
    let mut back_lines = Vec::new();
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Section {
        Header,
        Front,
        Back,
    }
    let mut section = Section::Header;
    let mut saw_front = false;
    let mut saw_back = false;

    for raw_line in text.lines() {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if section == Section::Header && line.trim() == "--- FRONT ---" {
            section = Section::Front;
            saw_front = true;
            continue;
        }
        if section == Section::Front && line.trim() == "--- BACK ---" {
            section = Section::Back;
            saw_back = true;
            continue;
        }
        match section {
            Section::Header => {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                if let Some(rest) = trimmed.strip_prefix("deck:") {
                    let value = rest.trim();
                    if !value.is_empty() {
                        deck_raw = Some(value.to_owned());
                    }
                } else if let Some(rest) = trimmed.strip_prefix("tags:") {
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
    if !saw_back {
        return Err("missing `--- BACK ---` marker".into());
    }

    let front = trim_surrounding_newlines(&front_lines.join("\n"));
    if front.trim().is_empty() {
        return Ok(None);
    }
    let back = trim_surrounding_newlines(&back_lines.join("\n"));
    let tags = normalize_user_tags(
        tags_raw
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(str::to_owned),
    );
    Ok(Some(ParsedDraftEdit {
        deck: deck_raw,
        front,
        back,
        tags,
    }))
}

fn trim_surrounding_newlines(text: &str) -> String {
    text.trim_matches(|ch| ch == '\n' || ch == '\r').to_owned()
}

pub fn resolve_editor_command(
    visual: Option<&str>,
    editor: Option<&str>,
) -> Result<Vec<String>, String> {
    let raw = visual
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| editor.map(str::trim).filter(|value| !value.is_empty()))
        .unwrap_or("vi");
    split_editor_command(raw)
}

/// Split an `$VISUAL` / `$EDITOR` command string honoring POSIX single/double quotes and
/// backslash escapes, while preserving direct unquoted executable paths that contain spaces.
pub fn split_editor_command(raw: &str) -> Result<Vec<String>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("EDITOR is empty".into());
    }
    if Path::new(trimmed).is_file() {
        return Ok(vec![trimmed.to_owned()]);
    }
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    let mut in_single = false;
    let mut in_double = false;
    let mut chars = trimmed.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_single {
            if ch == '\'' {
                in_single = false;
            } else {
                current.push(ch);
            }
            continue;
        }
        if in_double {
            match ch {
                '"' => in_double = false,
                '\\' => match chars.peek().copied() {
                    Some(next @ ('"' | '\\' | '$' | '`')) => {
                        chars.next();
                        current.push(next);
                    }
                    _ => current.push('\\'),
                },
                other => current.push(other),
            }
            continue;
        }
        match ch {
            '\'' => {
                in_single = true;
                in_token = true;
            }
            '"' => {
                in_double = true;
                in_token = true;
            }
            #[cfg(not(windows))]
            '\\' => {
                in_token = true;
                if let Some(escaped) = chars.next() {
                    current.push(escaped);
                } else {
                    return Err("unterminated escape in EDITOR".into());
                }
            }
            c if c.is_whitespace() => {
                if in_token {
                    parts.push(std::mem::take(&mut current));
                    in_token = false;
                }
            }
            other => {
                in_token = true;
                current.push(other);
            }
        }
    }
    if in_single || in_double {
        return Err("unterminated quote in EDITOR".into());
    }
    if in_token {
        parts.push(current);
    }
    if parts.first().is_none_or(String::is_empty) {
        return Err("EDITOR is empty".into());
    }
    Ok(parts)
}

pub fn edit_in_external_editor(initial_content: &str) -> Result<String, String> {
    let visual = env::var("VISUAL").ok();
    let editor = env::var("EDITOR").ok();
    let parts = resolve_editor_command(visual.as_deref(), editor.as_deref())?;
    let (program, args) = parts
        .split_first()
        .ok_or_else(|| "EDITOR is empty".to_owned())?;

    let mut temp_file = tempfile::Builder::new()
        .prefix("rem-draft-")
        .suffix(".md")
        .tempfile()
        .map_err(|error| error.to_string())?;
    temp_file
        .write_all(initial_content.as_bytes())
        .and_then(|()| temp_file.flush())
        .map_err(|error| error.to_string())?;
    let temp_path = temp_file.into_temp_path();

    let status = std::process::Command::new(program)
        .args(args)
        .arg(&temp_path)
        .status()
        .map_err(|error| format!("failed to launch {program}: {error}"))?;
    if !status.success() {
        return Err(format!("{program} exited with status {status}"));
    }
    fs::read_to_string(&temp_path).map_err(|error| error.to_string())
}
