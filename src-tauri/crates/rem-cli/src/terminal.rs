use std::io::{IsTerminal, Stdout};

use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
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
        enable_raw_mode().map_err(terminal_error)?;
        let mut stdout = std::io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen, Hide) {
            let _ = disable_raw_mode();
            return Err(terminal_error(error));
        }
        let backend = CrosstermBackend::new(stdout);
        match Terminal::new(backend) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                let _ = disable_raw_mode();
                let _ = execute!(std::io::stdout(), Show, LeaveAlternateScreen);
                Err(terminal_error(error))
            }
        }
    }

    fn suspend_for<T>(&mut self, action: impl FnOnce() -> T) -> Result<T, CliError> {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), Show, LeaveAlternateScreen);
        let result = action();
        enable_raw_mode().map_err(terminal_error)?;
        execute!(self.terminal.backend_mut(), EnterAlternateScreen, Hide)
            .map_err(terminal_error)?;
        self.terminal.clear().map_err(terminal_error)?;
        Ok(result)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), Show, LeaveAlternateScreen);
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
                Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                    KeyCode::Char('q') => DraftInput::Quit,
                    KeyCode::Char(' ') | KeyCode::Enter => DraftInput::Reveal,
                    KeyCode::Char('a') => DraftInput::Accept,
                    KeyCode::Char('r') => DraftInput::Reject,
                    KeyCode::Char('e') => DraftInput::Edit,
                    KeyCode::Char('n') | KeyCode::Tab => DraftInput::Skip,
                    KeyCode::Up | KeyCode::Char('k') => DraftInput::ScrollUp,
                    KeyCode::Down | KeyCode::Char('j') => DraftInput::ScrollDown,
                    _ => continue,
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
        session
            .terminal
            .draw(|frame| draw(frame, &app))
            .map_err(terminal_error)?;
        let input = match event::read().map_err(terminal_error)? {
            Event::Resize(_, _) => continue,
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Char('q') => StudyInput::Quit,
                KeyCode::Char(' ') | KeyCode::Enter => StudyInput::Reveal,
                KeyCode::Char('1') => StudyInput::Grade(Grade::Again),
                KeyCode::Char('2') => StudyInput::Grade(Grade::Hard),
                KeyCode::Char('3') => StudyInput::Grade(Grade::Good),
                KeyCode::Char('4') => StudyInput::Grade(Grade::Easy),
                KeyCode::Char('u') => StudyInput::Undo,
                KeyCode::Char('z') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    StudyInput::Undo
                }
                KeyCode::Char('s') => StudyInput::Suspend,
                KeyCode::Up | KeyCode::Char('k') => StudyInput::ScrollUp,
                KeyCode::Down | KeyCode::Char('j') => StudyInput::ScrollDown,
                _ => continue,
            },
            _ => continue,
        };
        if app.handle(collection, input, now_millis())? == StudyControl::Quit {
            break;
        }
    }
    Ok(())
}
