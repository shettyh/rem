use std::io::{IsTerminal, Stdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use rem_cli::draft_tui::{draw_draft, DraftApp, DraftControl, DraftInput};
use rem_cli::editor::edit_in_external_editor;
use rem_cli::tui::{draw, StudyApp, StudyControl, StudyInput, StudyScope};
use rem_core::{Collection, Grade, StudyRequest};

use crate::cli::{ReviewDraftsArgs, StudyArgs};
use crate::output::{input_error, now_millis, resolve_deck, terminal_error, CliError};

static TERMINAL_ACTIVE: AtomicBool = AtomicBool::new(false);
static PANIC_HOOK_ONCE: Once = Once::new();

fn install_panic_restore_hook() {
    PANIC_HOOK_ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if TERMINAL_ACTIVE.swap(false, Ordering::SeqCst) {
                let _ = disable_raw_mode();
                let _ = execute!(std::io::stdout(), Show, LeaveAlternateScreen);
            }
            previous(info);
        }));
    });
}

pub fn study(collection: &Collection, args: StudyArgs) -> Result<(), CliError> {
    require_interactive_terminal("rem study")?;

    let (request, scope) = match args.deck {
        Some(reference) => {
            let deck = resolve_deck(collection, &reference)?;
            (StudyRequest::deck(deck.id), StudyScope::Deck(deck.name))
        }
        None => (StudyRequest::all(), StudyScope::AllDecks),
    };
    let app = StudyApp::start(collection, request, scope, now_millis())?;
    run_study_terminal(collection, app)
}

pub fn review_drafts(collection: &Collection, args: ReviewDraftsArgs) -> Result<(), CliError> {
    require_interactive_terminal("rem draft review")?;

    let filter_deck = match args.deck {
        Some(reference) => Some(resolve_deck(collection, &reference)?),
        None => None,
    };
    let app = DraftApp::start(
        collection,
        filter_deck
            .as_ref()
            .map(|deck| (deck.id.as_str(), deck.name.as_str())),
    )?;
    run_draft_terminal(collection, app)
}

fn require_interactive_terminal(command: &str) -> Result<(), CliError> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(input_error(
            "interactive_terminal_required",
            format!("{command} requires an interactive terminal"),
        ));
    }
    Ok(())
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    fn enter() -> Result<Self, CliError> {
        install_panic_restore_hook();
        enable_raw_mode().map_err(terminal_error)?;
        let mut stdout = std::io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen, Hide) {
            let _ = disable_raw_mode();
            return Err(terminal_error(error));
        }
        let backend = CrosstermBackend::new(stdout);
        match Terminal::new(backend) {
            Ok(terminal) => {
                TERMINAL_ACTIVE.store(true, Ordering::SeqCst);
                Ok(Self { terminal })
            }
            Err(error) => {
                let _ = disable_raw_mode();
                let _ = execute!(std::io::stdout(), Show, LeaveAlternateScreen);
                Err(terminal_error(error))
            }
        }
    }

    fn suspend_for<T>(&mut self, action: impl FnOnce() -> T) -> Result<T, CliError> {
        TERMINAL_ACTIVE.store(false, Ordering::SeqCst);
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), Show, LeaveAlternateScreen);
        let result = action();
        enable_raw_mode().map_err(terminal_error)?;
        if let Err(error) = execute!(self.terminal.backend_mut(), EnterAlternateScreen, Hide) {
            let _ = disable_raw_mode();
            return Err(terminal_error(error));
        }
        TERMINAL_ACTIVE.store(true, Ordering::SeqCst);
        self.terminal.clear().map_err(terminal_error)?;
        Ok(result)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        TERMINAL_ACTIVE.store(false, Ordering::SeqCst);
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), Show, LeaveAlternateScreen);
    }
}

fn is_ctrl_c(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
}

fn has_cmd_modifiers(key: KeyEvent) -> bool {
    key.modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
}

fn map_draft_key(key: KeyEvent) -> Option<DraftInput> {
    if is_ctrl_c(key) {
        return Some(DraftInput::Quit);
    }
    if has_cmd_modifiers(key) {
        return None;
    }
    match key.code {
        KeyCode::Char(c) => match c.to_ascii_lowercase() {
            'q' => Some(DraftInput::Quit),
            ' ' => Some(DraftInput::Reveal),
            'a' => Some(DraftInput::Accept),
            'r' => Some(DraftInput::Reject),
            'e' => Some(DraftInput::Edit),
            'n' => Some(DraftInput::Skip),
            'k' => Some(DraftInput::ScrollUp),
            'j' => Some(DraftInput::ScrollDown),
            _ => None,
        },
        KeyCode::Enter => Some(DraftInput::Reveal),
        KeyCode::Tab => Some(DraftInput::Skip),
        KeyCode::Up => Some(DraftInput::ScrollUp),
        KeyCode::Down => Some(DraftInput::ScrollDown),
        _ => None,
    }
}

fn map_study_key(key: KeyEvent) -> Option<StudyInput> {
    if is_ctrl_c(key) {
        return Some(StudyInput::Quit);
    }
    if key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::ALT)
        && matches!(key.code, KeyCode::Char('z') | KeyCode::Char('Z'))
    {
        return Some(StudyInput::Undo);
    }
    if has_cmd_modifiers(key) {
        return None;
    }
    match key.code {
        KeyCode::Char(c) => match c.to_ascii_lowercase() {
            'q' => Some(StudyInput::Quit),
            ' ' => Some(StudyInput::Reveal),
            '1' => Some(StudyInput::Grade(Grade::Again)),
            '2' => Some(StudyInput::Grade(Grade::Hard)),
            '3' => Some(StudyInput::Grade(Grade::Good)),
            '4' => Some(StudyInput::Grade(Grade::Easy)),
            'u' => Some(StudyInput::Undo),
            's' => Some(StudyInput::Suspend),
            'k' => Some(StudyInput::ScrollUp),
            'j' => Some(StudyInput::ScrollDown),
            _ => None,
        },
        KeyCode::Enter => Some(StudyInput::Reveal),
        KeyCode::Up => Some(StudyInput::ScrollUp),
        KeyCode::Down => Some(StudyInput::ScrollDown),
        _ => None,
    }
}

fn run_draft_terminal(collection: &Collection, mut app: DraftApp) -> Result<(), CliError> {
    {
        let mut session = TerminalSession::enter()?;
        loop {
            session
                .terminal
                .draw(|frame| draw_draft(frame, &app))
                .map_err(terminal_error)?;
            let input = match event::read().map_err(terminal_error)? {
                Event::Resize(_, _) => continue,
                Event::Key(key) if key.kind != KeyEventKind::Release => match map_draft_key(key) {
                    Some(input) => input,
                    None => continue,
                },
                _ => continue,
            };
            match app.handle(collection, input, now_millis())? {
                DraftControl::Continue => {}
                DraftControl::Quit => break,
                DraftControl::EditInEditor => {
                    if let Some(template) = app.format_current_for_editor() {
                        let edited = session.suspend_for(|| edit_in_external_editor(&template))?;
                        match edited {
                            Ok(text) => app.apply_editor_output(&text),
                            Err(message) => {
                                app.set_status_message(format!("Editor failed: {message}"));
                            }
                        }
                    }
                }
            }
        }
    }

    let summary = app.summary();
    println!(
        "Draft triage complete: accepted={}, existing={}, rejected={}, remaining={}",
        summary.accepted, summary.existing, summary.rejected, summary.remaining
    );
    Ok(())
}

fn run_study_terminal(collection: &Collection, mut app: StudyApp) -> Result<(), CliError> {
    let mut session = TerminalSession::enter()?;

    loop {
        app.tick(now_millis());
        session
            .terminal
            .draw(|frame| draw(frame, &app))
            .map_err(terminal_error)?;
        if app.view().show_timer
            && app.view().current.is_some()
            && !app.view().revealed
            && !event::poll(std::time::Duration::from_millis(250)).map_err(terminal_error)?
        {
            continue;
        }
        let input = match event::read().map_err(terminal_error)? {
            Event::Resize(_, _) => continue,
            Event::Key(key) if key.kind != KeyEventKind::Release => match map_study_key(key) {
                Some(input) => input,
                None => continue,
            },
            _ => continue,
        };
        if app.handle(collection, input, now_millis())? == StudyControl::Quit {
            break;
        }
    }
    Ok(())
}
