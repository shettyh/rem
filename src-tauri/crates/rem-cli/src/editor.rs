use std::env;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

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
    Ok(Some(ParsedDraftEdit {
        deck: deck_raw,
        front,
        back,
        tags,
    }))
}

pub fn edit_in_external_editor(initial_content: &str) -> Result<String, String> {
    let editor = env::var("VISUAL")
        .or_else(|_| env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_owned());
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let temp_path = env::temp_dir().join(format!("rem-draft-{}-{nonce}.md", std::process::id()));
    fs::write(&temp_path, initial_content).map_err(|error| error.to_string())?;
    let mut parts = editor.split_whitespace();
    let Some(program) = parts.next() else {
        let _ = fs::remove_file(&temp_path);
        return Err("EDITOR is empty".into());
    };
    let status = match std::process::Command::new(program)
        .args(parts)
        .arg(&temp_path)
        .status()
    {
        Ok(status) => status,
        Err(error) => {
            let _ = fs::remove_file(&temp_path);
            return Err(format!("failed to launch {program}: {error}"));
        }
    };
    let updated = fs::read_to_string(&temp_path).map_err(|error| error.to_string());
    let _ = fs::remove_file(&temp_path);
    if !status.success() {
        return Err(format!("{program} exited with status {status}"));
    }
    updated
}
