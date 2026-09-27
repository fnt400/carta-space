#[cfg(not(test))]
use arboard::Clipboard;
use carta_core::{Archive, LeapDirection, SyncOutcome};
use carta_tui::app::{AppMode, View};
use carta_tui::editor::{visual_ranges, Cursor};
use carta_tui::help::{documents as help_documents, HelpKind};
use carta_tui::session::{
    load_last_archive, load_session, save_last_archive, save_session, state_root,
};
use carta_tui::App;
use chrono::{Datelike, Local, Weekday};
use clap::Parser;
use crossterm::cursor::{Hide, Show};
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags,
    ModifierKeyCode, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Terminal;
use std::backtrace::Backtrace;
use std::error::Error;
use std::io::{self, Stdout};
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthChar;

#[derive(Parser)]
#[command(name = "carta-tui", version, about = "Carta Space writing environment")]
struct Args {
    /// Archive directory. Without it, the device-local last Archive is resumed.
    path: Option<PathBuf>,
    /// Create the Archive at PATH before opening it.
    #[arg(long, requires = "path")]
    create: bool,
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    enhancements: bool,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut enhancements = false;
        let result = (|| {
            let mut stdout = io::stdout();
            execute!(stdout, EnterAlternateScreen, Hide)?;
            if supports_keyboard_enhancement().unwrap_or(false) {
                let flags = KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                    | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                    | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES;
                execute!(stdout, PushKeyboardEnhancementFlags(flags))?;
                enhancements = true;
            }
            Ok(Self {
                terminal: Terminal::new(CrosstermBackend::new(stdout))?,
                enhancements,
            })
        })();
        if result.is_err() {
            if enhancements {
                let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
            }
            let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
            let _ = disable_raw_mode();
        }
        result
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.enhancements {
            let _ = execute!(self.terminal.backend_mut(), PopKeyboardEnhancementFlags);
        }
        let _ = execute!(self.terminal.backend_mut(), Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

fn main() {
    let panic_report = Arc::new(Mutex::new(String::new()));
    let report = Arc::clone(&panic_report);
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("unknown panic payload");
        let location = info.location().map_or_else(
            || "unknown location".to_owned(),
            |location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            },
        );
        let backtrace = Backtrace::capture();
        let mut diagnostic = format!("carta-tui panicked at {location}:\n{payload}");
        if backtrace.status() == std::backtrace::BacktraceStatus::Captured {
            diagnostic.push_str(&format!("\n\nStack backtrace:\n{backtrace}"));
        }
        if let Ok(mut stored) = report.lock() {
            *stored = diagnostic;
        }
    }));

    let outcome = panic::catch_unwind(AssertUnwindSafe(run));
    panic::set_hook(previous_hook);
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            eprintln!("carta-tui: {error}");
            std::process::exit(1);
        }
        Err(_) => {
            let diagnostic = panic_report
                .lock()
                .map(|report| report.clone())
                .unwrap_or_else(|_| "carta-tui panicked (diagnostic unavailable)".to_owned());
            eprintln!("{diagnostic}");
            std::process::exit(101);
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let state = state_root()?;
    let path = match args.path {
        Some(path) => path,
        None => {
            load_last_archive(&state)?.ok_or("no last Archive; pass a path or use --create PATH")?
        }
    };
    let mut archive = if args.create {
        Archive::create(&path)?
    } else {
        Archive::open(&path)?
    };
    let startup_sync_status = if archive.is_dirty()? {
        None
    } else {
        match archive.sync() {
            Ok(report) if report.outcome() == SyncOutcome::Conflict => Some(
                "Sync conflict · local and remote histories preserved; resolution required"
                    .to_owned(),
            ),
            Ok(_) => None,
            Err(error) => Some(format!("Sync unavailable: {error}")),
        }
    };
    let canonical = archive
        .root()
        .canonicalize()
        .unwrap_or_else(|_| archive.root().to_path_buf());
    save_last_archive(&state, &canonical)?;
    let session = load_session(&state, archive.metadata().archive_id())?;
    let archive_id = archive.metadata().archive_id();
    let mut app = App::open(archive, session.as_ref(), Instant::now())?;
    if let Some(status) = startup_sync_status {
        app.status = status;
    }
    let mut terminal = TerminalGuard::enter()?;
    if !terminal.enhancements {
        if !app.status.is_empty() {
            app.status.push_str(" · ");
        }
        app.status
            .push_str("Compatibility keyboard mode: use palette LEAP commands");
    }
    let mut dispatcher = Dispatcher::default();
    let mut last_session_save = Instant::now();

    while !app.quit {
        terminal.terminal.draw(|frame| draw(frame, &mut app))?;
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if let Err(error) =
                    handle_key(&mut app, &mut dispatcher, key, terminal.enhancements)
                {
                    app.status = error.to_string();
                    app.mode = AppMode::Editing;
                    let _ = app.reveal_conflicts();
                }
            }
        }
        if let Err(error) = app.tick(Instant::now()) {
            app.status = error.to_string();
        }
        if last_session_save.elapsed() >= Duration::from_secs(1) {
            save_session(&state, archive_id, &app.session())?;
            last_session_save = Instant::now();
        }
    }
    save_session(&state, archive_id, &app.session())?;
    Ok(())
}

#[derive(Default)]
struct Dispatcher {
    pending_leap: Option<PendingLeap>,
    active_leap: Option<PendingLeap>,
    suppressed_leap_releases: u8,
    right_control_held: bool,
    clipboard: ClipboardBridge,
}

#[derive(Default)]
struct ClipboardBridge {
    #[cfg(not(test))]
    system: Option<Clipboard>,
    #[cfg(test)]
    text: Option<String>,
}

impl ClipboardBridge {
    #[cfg(not(test))]
    fn system(&mut self) -> Result<&mut Clipboard, String> {
        if self.system.is_none() {
            self.system = Some(Clipboard::new().map_err(|error| error.to_string())?);
        }
        self.system
            .as_mut()
            .ok_or_else(|| "system clipboard unavailable".to_owned())
    }

    fn set_text(&mut self, text: String) -> Result<(), String> {
        #[cfg(not(test))]
        {
            self.system()?
                .set_text(text)
                .map_err(|error| error.to_string())
        }
        #[cfg(test)]
        {
            self.text = Some(text);
            Ok(())
        }
    }

    fn get_text(&mut self) -> Result<String, String> {
        #[cfg(not(test))]
        {
            self.system()?.get_text().map_err(|error| error.to_string())
        }
        #[cfg(test)]
        {
            self.text
                .clone()
                .ok_or_else(|| "system clipboard has no text".to_owned())
        }
    }
}

#[derive(Clone, Copy)]
struct PendingLeap {
    direction: LeapDirection,
    key: ModifierKeyCode,
}

fn handle_key(
    app: &mut App,
    dispatcher: &mut Dispatcher,
    key: KeyEvent,
    enhanced: bool,
) -> Result<(), Box<dyn Error>> {
    if key.kind == KeyEventKind::Release {
        if let KeyCode::Modifier(released) = key.code {
            if released == ModifierKeyCode::RightControl {
                dispatcher.right_control_held = false;
                return Ok(());
            }
            if matches!(
                released,
                ModifierKeyCode::LeftControl | ModifierKeyCode::LeftAlt
            ) && dispatcher.suppressed_leap_releases > 0
            {
                dispatcher.suppressed_leap_releases -= 1;
                return Ok(());
            }
            if dispatcher
                .pending_leap
                .is_some_and(|pending| pending.key == released)
            {
                let pending = dispatcher.pending_leap.take().unwrap();
                app.cat_tap_leap(pending.direction);
            } else if dispatcher
                .active_leap
                .is_some_and(|active| active.key == released)
            {
                dispatcher.active_leap = None;
                if matches!(app.mode, AppMode::Leap { .. }) {
                    app.end_leap();
                }
            }
        }
        return Ok(());
    }
    if key.kind != KeyEventKind::Press && key.kind != KeyEventKind::Repeat {
        return Ok(());
    }

    if dispatcher.right_control_held
        && dispatcher.active_leap.is_none()
        && enhanced
        && key.kind == KeyEventKind::Press
    {
        if let KeyCode::Modifier(key @ (ModifierKeyCode::LeftControl | ModifierKeyCode::LeftAlt)) =
            key.code
        {
            dispatcher.pending_leap = None;
            dispatcher.active_leap = None;
            dispatcher.suppressed_leap_releases =
                dispatcher.suppressed_leap_releases.saturating_add(1);
            let direction = if key == ModifierKeyCode::LeftControl {
                LeapDirection::Backward
            } else {
                LeapDirection::Forward
            };
            app.leap_again(direction);
            return Ok(());
        }
    }

    if dispatcher.right_control_held
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('z' | 'Z' | 'r' | 'R'))
    {
        dispatcher.pending_leap = None;
        if matches!(app.mode, AppMode::Editing)
            && matches!(app.view, View::Chronological(_) | View::Work(_))
            && !app.collapsed
        {
            let command = if matches!(key.code, KeyCode::Char('z' | 'Z')) {
                carta_tui::Command::Undo
            } else {
                carta_tui::Command::Redo
            };
            app.execute(command)?;
        }
        return Ok(());
    }

    if dispatcher.right_control_held
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('c' | 'C'))
    {
        dispatcher.pending_leap = None;
        if matches!(app.mode, AppMode::Editing)
            && matches!(app.view, View::Chronological(_) | View::Work(_))
            && !app.collapsed
        {
            if app.editor.cat_highlight().is_some() {
                app.copy_cat_highlight();
            } else {
                match dispatcher.clipboard.get_text() {
                    Ok(text) if text.is_empty() => {
                        app.status = "System clipboard is empty".into();
                    }
                    Ok(text) => {
                        let text = normalize_clipboard_text(&text);
                        if app.cat_insert(&text) {
                            app.edited(Instant::now());
                            app.status.clear();
                        }
                    }
                    Err(error) => {
                        app.status = format!("System clipboard unavailable: {error}");
                    }
                }
            }
        }
        return Ok(());
    }

    if enhanced {
        if let Some(pending) = dispatcher.pending_leap {
            let handled = match (pending.direction, key.code) {
                (LeapDirection::Backward, KeyCode::Home)
                | (LeapDirection::Forward, KeyCode::End) => {
                    app.leap_document_boundary(pending.direction);
                    true
                }
                (LeapDirection::Backward, KeyCode::PageUp)
                | (LeapDirection::Forward, KeyCode::PageDown) => {
                    app.leap_view_boundary(pending.direction);
                    true
                }
                _ => false,
            };
            if handled {
                dispatcher.pending_leap = None;
                dispatcher.active_leap = Some(pending);
                return Ok(());
            }
        }
    }

    if key.modifiers.contains(KeyModifiers::CONTROL)
        && (!enhanced
            || dispatcher.right_control_held
            || (dispatcher.pending_leap.is_none() && dispatcher.active_leap.is_none()))
    {
        let editable = matches!(app.mode, AppMode::Editing)
            && matches!(app.view, View::Chronological(_) | View::Work(_));
        match key.code {
            KeyCode::PageUp if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                dispatcher.pending_leap = None;
                if editable {
                    app.editor.move_document(false);
                    app.cat_navigation();
                }
                return Ok(());
            }
            KeyCode::PageDown if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                dispatcher.pending_leap = None;
                if editable {
                    app.editor.move_document(true);
                    app.cat_navigation();
                }
                return Ok(());
            }
            KeyCode::Home if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                dispatcher.pending_leap = None;
                if editable {
                    app.editor.document_home(false);
                    app.cat_navigation();
                }
                return Ok(());
            }
            KeyCode::End if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                dispatcher.pending_leap = None;
                if editable {
                    app.editor.document_end(false);
                    app.cat_navigation();
                }
                return Ok(());
            }
            _ => {}
        }
    }

    if enhanced && key.kind == KeyEventKind::Press {
        match key.code {
            KeyCode::Modifier(ModifierKeyCode::RightControl) => {
                dispatcher.right_control_held = true;
                if let Some(active) = dispatcher.active_leap {
                    if matches!(app.mode, AppMode::Leap { palette: false, .. }) {
                        app.leap_again_active();
                    } else {
                        app.leap_again_active_structural(active.direction);
                    }
                }
                return Ok(());
            }
            KeyCode::Modifier(
                ModifierKeyCode::LeftShift
                | ModifierKeyCode::RightShift
                | ModifierKeyCode::RightAlt,
            ) => {
                return Ok(());
            }
            KeyCode::Modifier(key @ (ModifierKeyCode::LeftControl | ModifierKeyCode::LeftAlt)) => {
                if let Some(active) = dispatcher.active_leap {
                    if active.key != key {
                        dispatcher.active_leap = None;
                        dispatcher.suppressed_leap_releases = 2;
                        if matches!(app.mode, AppMode::Leap { .. }) {
                            app.end_leap();
                        }
                        if app.extend_last_leap_highlight() {
                            if let Some(text) = app.editor.selected_text() {
                                if let Err(error) = dispatcher.clipboard.set_text(text) {
                                    app.status = format!(
                                        "Cat highlight active; clipboard unavailable: {error}"
                                    );
                                }
                            }
                        }
                    }
                    return Ok(());
                }
                if let Some(pending) = dispatcher.pending_leap {
                    if pending.key != key {
                        dispatcher.pending_leap = None;
                        dispatcher.suppressed_leap_releases = 2;
                        if app.extend_last_leap_highlight() {
                            if let Some(text) = app.editor.selected_text() {
                                if let Err(error) = dispatcher.clipboard.set_text(text) {
                                    app.status = format!(
                                        "Cat highlight active; clipboard unavailable: {error}"
                                    );
                                }
                            }
                        }
                        return Ok(());
                    }
                    return Ok(());
                }
                let direction = if key == ModifierKeyCode::LeftControl {
                    LeapDirection::Backward
                } else {
                    LeapDirection::Forward
                };
                dispatcher.pending_leap = Some(PendingLeap { direction, key });
                return Ok(());
            }
            _ => {}
        }
    }

    if dispatcher.pending_leap.is_some() && matches!(key.code, KeyCode::Modifier(_)) {
        dispatcher.pending_leap = None;
        return Ok(());
    }
    if let Some(pending) = dispatcher.pending_leap.take() {
        dispatcher.active_leap = Some(pending);
        app.start_leap(pending.direction, false);
    }

    if let AppMode::Palette { query, selected } = &app.mode {
        let query_value = query.clone();
        let selected_value = *selected;
        let commands = app.palette_commands(&query_value);
        let mut execute = None;
        if let AppMode::Palette { query, selected } = &mut app.mode {
            match key.code {
                KeyCode::Esc => app.cancel_mode(),
                KeyCode::Char(c) => {
                    query.push(c);
                    *selected = 0;
                }
                KeyCode::Backspace => {
                    query.pop();
                    *selected = 0;
                }
                KeyCode::Up => *selected = selected.saturating_sub(1),
                KeyCode::Down => *selected = (*selected + 1).min(commands.len().saturating_sub(1)),
                KeyCode::Enter => execute = commands.get(selected_value).copied(),
                _ => {}
            }
        }
        if let Some(command) = execute {
            app.mode = AppMode::Editing;
            app.execute(command)?;
        }
        return Ok(());
    }

    match &mut app.mode {
        AppMode::Leap { .. } => match key.code {
            KeyCode::Char(c) => app.leap_input(&c.to_string()),
            KeyCode::Backspace => app.leap_backspace(),
            KeyCode::Enter => {
                if matches!(app.mode, AppMode::Leap { palette: true, .. }) {
                    app.end_leap();
                } else {
                    app.leap_input("\n");
                }
            }
            KeyCode::Esc => app.cancel_leap(),
            _ => {}
        },
        AppMode::Palette { .. } => unreachable!(),
        AppMode::Prompt { input, cursor, .. } => match key.code {
            KeyCode::Esc => app.cancel_mode(),
            KeyCode::Enter => app.submit_prompt()?,
            KeyCode::Left => *cursor = previous_char_boundary(input, *cursor),
            KeyCode::Right => *cursor = next_char_boundary(input, *cursor),
            KeyCode::Home => *cursor = 0,
            KeyCode::End => *cursor = input.len(),
            KeyCode::Backspace => {
                let previous = previous_char_boundary(input, *cursor);
                if previous < *cursor {
                    input.replace_range(previous..*cursor, "");
                    *cursor = previous;
                }
            }
            KeyCode::Delete => {
                let next = next_char_boundary(input, *cursor);
                if next > *cursor {
                    input.replace_range(*cursor..next, "");
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                input.insert(*cursor, c);
                *cursor += c.len_utf8();
            }
            _ => {}
        },
        AppMode::Confirm { .. } => match key.code {
            KeyCode::Char('y' | 'Y') => app.submit_confirmation(true)?,
            KeyCode::Char('n' | 'N') | KeyCode::Esc | KeyCode::Enter => {
                app.submit_confirmation(false)?
            }
            _ => {}
        },
        AppMode::Selector {
            query,
            selected,
            choices,
            ..
        } => match key.code {
            KeyCode::Esc => app.cancel_mode(),
            KeyCode::Enter => app.submit_selector()?,
            KeyCode::Backspace => {
                query.pop();
                *selected = 0;
            }
            KeyCode::Char(c) => {
                query.push(c);
                *selected = 0;
            }
            KeyCode::Up => *selected = selected.saturating_sub(1),
            KeyCode::Down => {
                let len = choices
                    .iter()
                    .filter(|c| carta_tui::palette::matches(query, &c.label))
                    .count();
                *selected = (*selected + 1).min(len.saturating_sub(1));
            }
            _ => {}
        },
        AppMode::Editing => handle_normal(app, key)?,
    }
    Ok(())
}

fn previous_char_boundary(text: &str, cursor: usize) -> usize {
    text[..cursor.min(text.len())]
        .char_indices()
        .next_back()
        .map_or(0, |(index, _)| index)
}

fn next_char_boundary(text: &str, cursor: usize) -> usize {
    let cursor = cursor.min(text.len());
    text[cursor..]
        .chars()
        .next()
        .map_or(text.len(), |character| cursor + character.len_utf8())
}

fn normalize_clipboard_text(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn handle_normal(app: &mut App, key: KeyEvent) -> Result<(), Box<dyn Error>> {
    if key.code == KeyCode::Esc {
        app.open_palette();
        return Ok(());
    }
    if matches!(
        app.view,
        View::Search { .. }
            | View::History { .. }
            | View::WorkHistory { .. }
            | View::Trash { .. }
            | View::Conflicts { .. }
            | View::Help { .. }
    ) {
        match key.code {
            KeyCode::Up => app.move_list_selection(false),
            KeyCode::Down => app.move_list_selection(true),
            KeyCode::Enter => app.open_selected()?,
            _ => {}
        }
        return Ok(());
    }
    if app.collapsed {
        match key.code {
            KeyCode::Up | KeyCode::PageUp => {
                app.editor.move_document(false);
                app.cat_navigation();
            }
            KeyCode::Down | KeyCode::PageDown => {
                app.editor.move_document(true);
                app.cat_navigation();
            }
            KeyCode::Enter => app.execute(carta_tui::Command::ExpandView)?,
            _ => {}
        }
        return Ok(());
    }

    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let (terminal_width, height) = crossterm::terminal::size().unwrap_or((80, 24));
    let width = editor_width(terminal_width);
    let page = usize::from(height.saturating_sub(2).max(1));
    let changed = match key.code {
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.cat_insert(&c.to_string())
        }
        KeyCode::Enter => app.cat_insert_newline(),
        KeyCode::Tab if shift => {
            app.cat_navigation();
            app.editor.indent_less()
        }
        KeyCode::Tab => app.cat_insert("    "),
        KeyCode::Backspace => app.cat_backspace(),
        KeyCode::Delete => app.cat_erase(),
        KeyCode::Left => {
            app.editor.move_horizontal(false, false);
            app.cat_navigation();
            false
        }
        KeyCode::Right => {
            app.editor.move_horizontal(true, false);
            app.cat_navigation();
            false
        }
        KeyCode::Up => {
            app.editor.move_visual(false, width, false);
            app.cat_navigation();
            false
        }
        KeyCode::Down => {
            app.editor.move_visual(true, width, false);
            app.cat_navigation();
            false
        }
        KeyCode::Home => {
            app.editor.visual_home(width, false);
            app.cat_navigation();
            false
        }
        KeyCode::End => {
            app.editor.visual_end(width, false);
            app.cat_navigation();
            false
        }
        KeyCode::PageUp => {
            app.editor.page_visual(false, page, width, false);
            app.cat_navigation();
            false
        }
        KeyCode::PageDown => {
            app.editor.page_visual(true, page, width, false);
            app.cat_navigation();
            false
        }
        _ => false,
    };
    if changed {
        app.edited(Instant::now());
    }
    Ok(())
}

fn draw(frame: &mut ratatui::Frame<'_>, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());
    match &app.view {
        View::Chronological(_) | View::Work(_) => draw_editor(frame, app, editor_rect(chunks[0])),
        View::Search { query, selected } => {
            let items: Vec<_> = app
                .search_results
                .iter()
                .map(|r| {
                    ListItem::new(vec![
                        Line::from(r.label.clone()),
                        Line::styled(date_only(r.created), Style::default().fg(Color::DarkGray)),
                        Line::styled(r.context.clone(), Style::default().fg(Color::DarkGray)),
                    ])
                })
                .collect();
            draw_list(
                frame,
                chunks[0],
                format!("Search: {query}"),
                items,
                *selected,
            );
        }
        View::History { selected, .. } => {
            let items = app
                .history
                .iter()
                .map(|r| {
                    ListItem::new(format!(
                        "{}  {}",
                        local_timestamp(r.checkpoint().created()),
                        r.checkpoint().note().unwrap_or("checkpoint")
                    ))
                })
                .collect();
            let panes = Layout::vertical([Constraint::Percentage(35), Constraint::Percentage(65)])
                .split(chunks[0]);
            draw_list(frame, panes[0], "History".into(), items, *selected);
            let content = app
                .history
                .get(*selected)
                .map_or("", |revision| revision.document().content());
            frame.render_widget(
                Paragraph::new(content).block(Block::default().title("Historical Markdown")),
                panes[1],
            );
        }
        View::WorkHistory { selected, .. } => {
            let items = app
                .work_history
                .iter()
                .map(|snapshot| {
                    ListItem::new(format!(
                        "{}  {}  {} Documents",
                        local_timestamp(snapshot.checkpoint().created()),
                        snapshot.title(),
                        snapshot.document_ids().len()
                    ))
                })
                .collect();
            let panes = Layout::vertical([Constraint::Percentage(35), Constraint::Percentage(65)])
                .split(chunks[0]);
            draw_list(frame, panes[0], "Work History".into(), items, *selected);
            let content = app
                .work_history
                .get(*selected)
                .map_or_else(String::new, |snapshot| {
                    snapshot
                        .documents()
                        .iter()
                        .map(|document| document.content())
                        .collect::<Vec<_>>()
                        .join("\n────────────────\n")
                });
            frame.render_widget(
                Paragraph::new(content).block(Block::default().title("Historical Work")),
                panes[1],
            );
        }
        View::Trash { selected } => {
            let mut items = Vec::new();
            if let Some(trash) = &app.trash {
                items.extend(trash.documents().iter().map(|d| {
                    ListItem::new(format!(
                        "Document · {} · {}",
                        local_timestamp(d.created()),
                        d.label()
                    ))
                }));
                items.extend(trash.works().iter().map(|w| {
                    ListItem::new(format!(
                        "Work · {} · {}",
                        local_timestamp(w.created()),
                        w.title()
                    ))
                }));
            }
            draw_list(frame, chunks[0], "Trash".into(), items, *selected);
        }
        View::Conflicts { selected } => {
            let items: Vec<_> = app
                .conflicts
                .iter()
                .map(|conflict| match conflict {
                    carta_core::Conflict::Document(conflict) => ListItem::new(format!(
                        "Document · {} · {}{}",
                        conflict.detected(),
                        conflict.document(),
                        if conflict.external_missing() {
                            " · external file missing"
                        } else {
                            ""
                        }
                    )),
                    carta_core::Conflict::Work(conflict) => ListItem::new(format!(
                        "Work · {} · {}{}",
                        conflict.detected(),
                        conflict.work(),
                        if conflict.external_missing() {
                            " · external file missing"
                        } else {
                            ""
                        }
                    )),
                })
                .collect();
            let panes = Layout::vertical([Constraint::Percentage(35), Constraint::Percentage(65)])
                .split(chunks[0]);
            draw_list(
                frame,
                panes[0],
                "Preserved Conflicts".into(),
                items,
                *selected,
            );
            let preview = app.conflicts.get(*selected).map_or_else(String::new, |conflict| {
                match conflict {
                    carta_core::Conflict::Document(conflict) => format!(
                        "LOCAL\n-----\n{}\n\n{}\n--------\n{}",
                        String::from_utf8_lossy(conflict.local()),
                        if conflict.external_missing() { "LAST LOADED (EXTERNAL FILE MISSING)" } else { "EXTERNAL" },
                        String::from_utf8_lossy(conflict.external())
                    ),
                    carta_core::Conflict::Work(conflict) => format!(
                        "LOCAL WORK STRUCTURE\n--------------------\n{}\n\n{}\n-----------------------\n{}",
                        String::from_utf8_lossy(conflict.local()),
                        if conflict.external_missing() { "LAST LOADED STRUCTURE (EXTERNAL FILE MISSING)" } else { "EXTERNAL WORK STRUCTURE" },
                        String::from_utf8_lossy(conflict.external())
                    ),
                }
            });
            frame.render_widget(
                Paragraph::new(preview).block(Block::default().title("Variants (read-only)")),
                panes[1],
            );
        }
        View::Help { kind, selected } => {
            let documents = help_documents(*kind);
            let document = documents.get(*selected).or_else(|| documents.first());
            let (title, body) = document
                .map(|document| (document.title, document.body))
                .unwrap_or(("Carta Space Help", "No help document available."));
            frame.render_widget(
                Paragraph::new(body)
                    .wrap(Wrap { trim: false })
                    .block(Block::default().borders(Borders::ALL).title(title)),
                chunks[0],
            );
        }
    }
    frame.render_widget(
        Paragraph::new(rendered_status_line(app, usize::from(chunks[1].width)))
            .style(status_style(app)),
        chunks[1],
    );
    draw_mode(frame, app);
}

struct VisualLine {
    region: Option<usize>,
    start: usize,
    end: usize,
    text: String,
}

fn editor_width(terminal_width: u16) -> usize {
    usize::from(terminal_width.clamp(1, 80))
}

fn editor_rect(area: Rect) -> Rect {
    let width = area.width.min(80);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y,
        width,
        area.height,
    )
}

fn draw_editor(frame: &mut ratatui::Frame<'_>, app: &mut App, area: Rect) {
    let width = usize::from(area.width.max(1));
    let lines = visual_lines(app, width);
    let cursor = app.editor.cursor();
    let cursor_line = lines
        .iter()
        .rposition(|l| {
            l.region == Some(cursor.region) && cursor.byte >= l.start && cursor.byte <= l.end
        })
        .unwrap_or(0);
    let height = usize::from(area.height.max(1));
    let (scroll, top_padding) = anchored_viewport(cursor_line, height);
    app.scroll = scroll;
    let selection = app.cat_render_highlight();
    let selection_style = if app.editor.cat_highlight().is_some() {
        Style::default().bg(Color::Blue)
    } else {
        Style::default().bg(Color::DarkGray).fg(Color::White)
    };
    let mut rendered = vec![Line::raw(String::new()); top_padding];
    rendered.extend(
        lines
            .iter()
            .skip(app.scroll)
            .take(height.saturating_sub(top_padding))
            .map(|line| {
                if let Some(region) = line.region {
                    styled_line(line, region, selection, selection_style)
                } else {
                    Line::styled(line.text.clone(), Style::default().fg(Color::DarkGray))
                }
            }),
    );
    frame.render_widget(Paragraph::new(rendered), area);
    if matches!(app.mode, AppMode::Editing | AppMode::Leap { .. }) {
        if let Some(line) = lines.get(cursor_line) {
            let text =
                &app.editor.regions()[cursor.region].text[line.start..cursor.byte.min(line.end)];
            let x = text.chars().map(|c| c.width().unwrap_or(0)).sum::<usize>();
            frame.set_cursor_position((
                area.x + x.min(width.saturating_sub(1)) as u16,
                area.y + top_padding as u16 + cursor_line.saturating_sub(app.scroll) as u16,
            ));
        }
    }
}

fn anchored_viewport(cursor_line: usize, height: usize) -> (usize, usize) {
    let target_row = height.saturating_mul(2) / 3;
    (
        cursor_line.saturating_sub(target_row),
        target_row.saturating_sub(cursor_line),
    )
}

fn visual_lines(app: &App, width: usize) -> Vec<VisualLine> {
    let mut out = Vec::new();
    for (region_index, region) in app.editor.regions().iter().enumerate() {
        match &app.view {
            View::Chronological(_) => {
                if region_index > 0 {
                    out.push(generated_line(String::new()));
                }
                out.push(generated_line(chronological_separator(
                    app,
                    region.document,
                    width,
                )));
                out.push(generated_line(String::new()));
            }
            View::Work(_) if region_index > 0 => {
                for text in [String::new(), "─".repeat(width), String::new()] {
                    out.push(generated_line(text));
                }
            }
            _ => {}
        }

        let visible_rows = if app.collapsed { 3 } else { usize::MAX };
        for (start, end) in visual_ranges(&region.text, width)
            .into_iter()
            .take(visible_rows)
        {
            out.push(VisualLine {
                region: Some(region_index),
                start,
                end,
                text: region.text[start..end].into(),
            });
        }
    }
    out
}

fn generated_line(text: String) -> VisualLine {
    VisualLine {
        region: None,
        start: 0,
        end: 0,
        text,
    }
}

fn chronological_separator(app: &App, document: carta_core::DocumentId, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let date = app
        .archive
        .documents()
        .find(|info| info.id() == document)
        .map_or_else(
            || "senza data".to_owned(),
            |info| italian_date(info.created()),
        );
    let prefix = format!("── {date} ");
    if display_width(&prefix) >= width {
        return truncate_display(&prefix, width, false);
    }

    let memberships = membership_summary(app, document);
    let available = width.saturating_sub(display_width(&prefix));
    let suffix = if memberships.is_empty() || available < 5 {
        String::new()
    } else {
        let summary = truncate_display(&memberships, available.saturating_sub(4), true);
        format!(" {summary} ──")
    };
    let fill = width
        .saturating_sub(display_width(&prefix))
        .saturating_sub(display_width(&suffix));
    format!("{prefix}{}{suffix}", "─".repeat(fill))
}

fn membership_summary(app: &App, document: carta_core::DocumentId) -> String {
    let mut titles: Vec<_> = app
        .archive
        .works()
        .filter(|work| work.documents().contains(&document))
        .map(|work| work.title().to_owned())
        .collect();
    titles.sort_by_key(|title| title.to_lowercase());
    let more = titles.len() > 2;
    titles.truncate(2);
    let mut summary = titles.join(" - ");
    if more {
        if !summary.is_empty() {
            summary.push_str(" - ");
        }
        summary.push('…');
    }
    summary
}

fn italian_date(timestamp: carta_core::Timestamp) -> String {
    let date = timestamp.as_datetime().with_timezone(&Local);
    let weekday = match date.weekday() {
        Weekday::Mon => "lun",
        Weekday::Tue => "mar",
        Weekday::Wed => "mer",
        Weekday::Thu => "gio",
        Weekday::Fri => "ven",
        Weekday::Sat => "sab",
        Weekday::Sun => "dom",
    };
    let month = match date.month() {
        1 => "gen",
        2 => "feb",
        3 => "mar",
        4 => "apr",
        5 => "mag",
        6 => "giu",
        7 => "lug",
        8 => "ago",
        9 => "set",
        10 => "ott",
        11 => "nov",
        12 => "dic",
        _ => "?",
    };
    format!("{weekday} {:02} {month} {}", date.day(), date.year())
}

fn display_width(text: &str) -> usize {
    text.chars()
        .map(|character| character.width().unwrap_or(0))
        .sum()
}

fn truncate_display(text: &str, max_width: usize, ellipsis: bool) -> String {
    if display_width(text) <= max_width {
        return text.to_owned();
    }
    if max_width == 0 {
        return String::new();
    }
    let reserve = usize::from(ellipsis);
    let content_width = max_width.saturating_sub(reserve);
    let mut result = String::new();
    let mut used = 0;
    for character in text.chars() {
        let width = character.width().unwrap_or(0);
        if used + width > content_width {
            break;
        }
        result.push(character);
        used += width;
    }
    if ellipsis {
        result.push('…');
    }
    result
}

fn styled_line(
    line: &VisualLine,
    region: usize,
    selection: Option<(Cursor, Cursor)>,
    selection_style: Style,
) -> Line<'_> {
    let Some((start, end)) = selection else {
        return Line::raw(line.text.clone());
    };
    if region < start.region || region > end.region {
        return Line::raw(line.text.clone());
    }
    let selected_start = if region == start.region {
        start.byte
    } else {
        0
    }
    .clamp(line.start, line.end);
    let selected_end = if region == end.region {
        end.byte
    } else {
        usize::MAX
    }
    .clamp(line.start, line.end);
    if selected_start >= selected_end {
        return Line::raw(line.text.clone());
    }
    let text = &line.text;
    let a = selected_start - line.start;
    let b = selected_end - line.start;
    Line::from(vec![
        Span::raw(text[..a].to_owned()),
        Span::styled(text[a..b].to_owned(), selection_style),
        Span::raw(text[b..].to_owned()),
    ])
}

fn draw_list(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    title: String,
    items: Vec<ListItem<'_>>,
    selected: usize,
) {
    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().title(title))
            .highlight_style(
                Style::default()
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ),
        area,
        &mut state,
    );
}

fn draw_mode(frame: &mut ratatui::Frame<'_>, app: &App) {
    let area = centered(frame.area(), 70, 45);
    match &app.mode {
        AppMode::Palette { query, selected } => {
            let commands = app.palette_commands(query);
            let items: Vec<_> = commands.iter().map(|c| ListItem::new(c.label())).collect();
            frame.render_widget(Clear, area);
            let mut state = ListState::default().with_selected(Some(*selected));
            frame.render_stateful_widget(
                List::new(items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(format!("> {query}")),
                    )
                    .highlight_style(Style::default().bg(Color::Blue)),
                area,
                &mut state,
            );
        }
        AppMode::Selector {
            title,
            query,
            selected,
            choices,
            ..
        } => {
            let items: Vec<_> = choices
                .iter()
                .filter(|c| carta_tui::palette::matches(query, &c.label))
                .map(|c| ListItem::new(c.label.clone()))
                .collect();
            frame.render_widget(Clear, area);
            let mut state = ListState::default().with_selected(Some(*selected));
            frame.render_stateful_widget(
                List::new(items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(format!("{title}: {query}")),
                    )
                    .highlight_style(Style::default().bg(Color::Blue)),
                area,
                &mut state,
            );
        }
        AppMode::Confirm { title, details, .. } => {
            let height = (details.len() as u16 + 4).min(area.height.max(4));
            let prompt = Rect {
                x: area.x,
                y: area.y + area.height.saturating_sub(height) / 2,
                width: area.width,
                height,
            };
            frame.render_widget(Clear, prompt);
            let text = if details.is_empty() {
                "Press y to confirm; n or Esc to cancel.".to_owned()
            } else {
                format!(
                    "{}\n\nPress y to confirm; n or Esc to cancel.",
                    details.join("\n")
                )
            };
            frame.render_widget(
                Paragraph::new(text)
                    .style(Style::default().fg(Color::Red))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Red))
                            .title(title.as_str()),
                    ),
                prompt,
            );
        }
        AppMode::Prompt {
            title,
            input,
            cursor,
            details,
            ..
        } => {
            let height = (details.len() as u16 + 3).min(area.height.max(3));
            let prompt = Rect {
                x: area.x,
                y: area.y + area.height.saturating_sub(height) / 2,
                width: area.width,
                height,
            };
            frame.render_widget(Clear, prompt);
            frame.render_widget(
                Paragraph::new(format!(
                    "{}{}",
                    if details.is_empty() {
                        String::new()
                    } else {
                        format!("{}\n", details.join("\n"))
                    },
                    input
                ))
                .block(Block::default().borders(Borders::ALL).title(title.as_str())),
                prompt,
            );
            frame.set_cursor_position((
                prompt.x
                    + 1
                    + input[..(*cursor).min(input.len())]
                        .chars()
                        .map(|c| c.width().unwrap_or(0))
                        .sum::<usize>()
                        .min(prompt.width.saturating_sub(3) as usize) as u16,
                prompt.y + 1 + details.len() as u16,
            ));
        }
        AppMode::Leap { session, .. } => {
            let text = format!("LEAP {:?}: {}", session.direction(), session.query());
            let popup = Rect {
                x: frame.area().x,
                y: frame.area().bottom().saturating_sub(2),
                width: frame.area().width,
                height: 1,
            };
            frame.render_widget(
                Paragraph::new(text).style(Style::default().fg(Color::Yellow)),
                popup,
            );
        }
        AppMode::Editing => {}
    }
}

fn rendered_status_line(app: &App, width: usize) -> String {
    let left = status_line(app);
    if !app.status.is_empty() || !matches!(app.view, View::Chronological(_)) {
        return truncate_display(&left, width, true);
    }
    let right = app
        .editor
        .current_document()
        .map_or_else(String::new, |document| membership_summary(app, document));
    if right.is_empty() {
        return truncate_display(&left, width, true);
    }

    let right = truncate_display(&right, width.saturating_sub(1), true);
    let right_width = display_width(&right);
    let left_budget = width.saturating_sub(right_width.saturating_add(1));
    let left = truncate_display(&left, left_budget, true);
    let padding = width
        .saturating_sub(display_width(&left))
        .saturating_sub(right_width);
    format!("{left}{}{right}", " ".repeat(padding))
}

fn status_style(app: &App) -> Style {
    let View::Work(work) = &app.view else {
        return Style::default().bg(Color::DarkGray).fg(Color::White);
    };
    let Some(color) = app.archive.work(*work).and_then(|work| work.color()) else {
        return Style::default().bg(Color::DarkGray).fg(Color::White);
    };
    let Some((red, green, blue)) = parse_rgb(color) else {
        return Style::default().bg(Color::DarkGray).fg(Color::White);
    };
    let luminance =
        (299_u32 * u32::from(red) + 587_u32 * u32::from(green) + 114_u32 * u32::from(blue)) / 1000;
    let foreground = if luminance >= 150 {
        Color::Black
    } else {
        Color::White
    };
    Style::default()
        .bg(Color::Rgb(red, green, blue))
        .fg(foreground)
}

fn parse_rgb(color: &str) -> Option<(u8, u8, u8)> {
    let value = color.strip_prefix('#')?;
    if value.len() != 6 {
        return None;
    }
    Some((
        u8::from_str_radix(&value[0..2], 16).ok()?,
        u8::from_str_radix(&value[2..4], 16).ok()?,
        u8::from_str_radix(&value[4..6], 16).ok()?,
    ))
}

fn status_line(app: &App) -> String {
    if !app.status.is_empty() {
        return app.status.clone();
    }
    match &app.view {
        View::Chronological(v) => {
            format!(
                "{} · {} · {:04}-{:02} · {}",
                current_document_date(app),
                current_label(app),
                v.year(),
                v.month(),
                current_position(app)
            )
        }
        View::Work(id) => format!(
            "{} · {} · {} · {}",
            current_document_date(app),
            current_label(app),
            app.archive.work(*id).map_or("Work", |w| w.title()),
            current_position(app)
        ),
        View::Search { query, selected } => {
            let date = app
                .search_results
                .get(*selected)
                .map_or_else(|| "No date".to_owned(), |row| date_only(row.created));
            format!(
                "{} · Search: {} · {}/{}",
                date,
                query,
                if app.search_results.is_empty() {
                    0
                } else {
                    selected + 1
                },
                app.search_results.len()
            )
        }
        View::History { selected, .. } => app.history.get(*selected).map_or_else(
            || "History · 0/0".to_owned(),
            |revision| {
                format!(
                    "{} · History · {} · {}",
                    date_only(revision.checkpoint().created()),
                    revision.document().derived_label(),
                    list_position(*selected, app.history.len())
                )
            },
        ),
        View::WorkHistory { selected, .. } => app.work_history.get(*selected).map_or_else(
            || "Work History · 0/0".to_owned(),
            |snapshot| {
                format!(
                    "{} · Work History · {} · {}",
                    date_only(snapshot.checkpoint().created()),
                    snapshot.title(),
                    list_position(*selected, app.work_history.len())
                )
            },
        ),
        View::Trash { selected } => {
            let Some(trash) = &app.trash else {
                return "Trash · 0/0".to_owned();
            };
            let total = trash.documents().len() + trash.works().len();
            if let Some(document) = trash.documents().get(*selected) {
                format!(
                    "{} · Trash · Document: {} · {}",
                    date_only(document.created()),
                    document.label(),
                    list_position(*selected, total)
                )
            } else if let Some(work) = selected
                .checked_sub(trash.documents().len())
                .and_then(|index| trash.works().get(index))
            {
                format!(
                    "{} · Trash · Work: {} · {}",
                    date_only(work.created()),
                    work.title(),
                    list_position(*selected, total)
                )
            } else {
                "Trash · 0/0".to_owned()
            }
        }
        View::Conflicts { .. } => format!("Conflicts · {} preserved", app.conflicts.len()),
        View::Help { kind, selected } => {
            let documents = help_documents(*kind);
            let label = match kind {
                HelpKind::Cheatsheet => "Cheatsheet",
                HelpKind::Manual => "Manual",
            };
            let title = documents
                .get(*selected)
                .map_or("Help", |document| document.title);
            format!(
                "{label} · {title} · {}",
                list_position(*selected, documents.len())
            )
        }
    }
}
fn current_label(app: &App) -> String {
    app.editor
        .current_document()
        .and_then(|id| app.archive.read_document(id).ok())
        .map_or_else(|| "Empty View".into(), |d| d.derived_label())
}
fn current_document_date(app: &App) -> String {
    app.editor
        .current_document()
        .and_then(|id| app.archive.documents().find(|document| document.id() == id))
        .map_or_else(
            || "No date".to_owned(),
            |document| date_only(document.created()),
        )
}
fn current_position(app: &App) -> String {
    if app.editor.regions().is_empty() {
        "0/0".to_owned()
    } else {
        format!(
            "{}/{}",
            app.editor.cursor().region + 1,
            app.editor.regions().len()
        )
    }
}
fn local_timestamp(timestamp: carta_core::Timestamp) -> String {
    timestamp.as_datetime().with_timezone(&Local).to_rfc3339()
}

fn date_only(timestamp: carta_core::Timestamp) -> String {
    timestamp
        .as_datetime()
        .with_timezone(&Local)
        .format("%Y-%m-%d")
        .to_string()
}
fn list_position(selected: usize, total: usize) -> String {
    if total == 0 {
        "0/0".to_owned()
    } else {
        format!("{}/{}", selected + 1, total)
    }
}
fn centered(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use carta_tui::editor::Cursor;

    fn dispatch(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
        handle_key(
            app,
            &mut Dispatcher::default(),
            KeyEvent::new(code, modifiers),
            true,
        )
        .unwrap();
    }

    fn app_with_documents(contents: &[&str], work: bool) -> (tempfile::TempDir, App) {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("archive");
        let mut archive = Archive::create(&root).unwrap();
        let documents: Vec<_> = contents
            .iter()
            .map(|content| archive.create_document(content).unwrap())
            .collect();
        let view = if work {
            let id = archive.create_work("Work".into(), documents).unwrap();
            carta_tui::session::SavedView::Work { id }
        } else {
            let volume = archive.documents().next().unwrap().volume();
            carta_tui::session::SavedView::Chronological {
                year: volume.year(),
                month: volume.month(),
            }
        };
        let session = carta_tui::session::Session {
            view,
            position: None,
            work_positions: Default::default(),
            work_mru: Vec::new(),
        };
        let app = App::open(archive, Some(&session), Instant::now()).unwrap();
        (temporary, app)
    }

    #[test]
    fn editor_rect_is_centered_and_capped_at_eighty_columns() {
        assert_eq!(
            editor_rect(Rect::new(0, 0, 160, 20)),
            Rect::new(40, 0, 80, 20)
        );
        assert_eq!(
            editor_rect(Rect::new(0, 0, 70, 20)),
            Rect::new(0, 0, 70, 20)
        );
        assert_eq!(editor_width(160), 80);
        assert_eq!(editor_width(70), 70);
    }

    #[test]
    fn prompt_cursor_moves_on_utf8_character_boundaries() {
        let text = "aèz";
        assert_eq!(next_char_boundary(text, 0), 1);
        assert_eq!(next_char_boundary(text, 1), 3);
        assert_eq!(previous_char_boundary(text, 3), 1);
        assert_eq!(previous_char_boundary(text, 1), 0);
    }

    #[test]
    fn editor_viewport_keeps_cursor_at_two_thirds_with_top_padding() {
        assert_eq!(anchored_viewport(0, 21), (0, 14));
        assert_eq!(anchored_viewport(8, 21), (0, 6));
        assert_eq!(anchored_viewport(20, 21), (6, 0));
    }

    #[test]
    fn collapsed_view_shows_only_three_authored_rows_per_document() {
        let (_temporary, mut app) =
            app_with_documents(&["one\ntwo\nthree\nfour\nfive", "second"], false);
        app.collapsed = true;

        let lines = visual_lines(&app, 80);
        let first_rows = lines.iter().filter(|line| line.region == Some(0)).count();
        assert_eq!(first_rows, 3);
    }

    #[test]
    fn visual_lines_share_wrap_and_generate_three_work_boundary_rows() {
        let first = "word ".repeat(20);
        let (_temporary, app) = app_with_documents(&[first.as_str(), "second"], true);
        let lines = visual_lines(&app, 80);
        assert!(lines.iter().all(|line| display_width(&line.text) <= 80));
        let boundary = lines
            .windows(3)
            .position(|window| {
                window[0].region.is_none()
                    && window[0].text.is_empty()
                    && window[1].text == "─".repeat(80)
                    && window[2].text.is_empty()
            })
            .unwrap();
        assert!(lines[boundary].text.is_empty());
        assert_eq!(lines[boundary + 1].text, "─".repeat(80));
        assert!(lines[boundary + 2].text.is_empty());
        assert_eq!(app.editor.regions()[0].text, first);
    }

    #[test]
    fn chronological_headers_show_italian_date_and_up_to_two_work_memberships() {
        use std::str::FromStr;

        let timestamp = carta_core::Timestamp::from_str("2026-09-25T12:00:00+02:00").unwrap();
        assert_eq!(italian_date(timestamp), "ven 25 set 2026");

        let (_temporary, mut app) = app_with_documents(&["first", "second"], false);
        let document = app.editor.regions()[0].document;
        app.archive
            .create_work("Cinema".into(), vec![document])
            .unwrap();
        app.archive
            .create_work("Appunti".into(), vec![document])
            .unwrap();
        app.archive
            .create_work("Terzo".into(), vec![document])
            .unwrap();

        let lines = visual_lines(&app, 80);
        let header = lines
            .iter()
            .find(|line| line.region.is_none() && line.text.contains("Cinema"))
            .expect("chronological metadata separator");
        assert!(header.text.contains("Appunti"));
        assert!(header.text.contains('…'));
        assert!(display_width(&header.text) <= 80);

        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        let status = rendered_status_line(&app, 80);
        assert!(
            status.ends_with("Appunti - Cinema - …") || status.ends_with("Cinema - Appunti - …")
        );
    }

    #[test]
    fn displayed_dates_use_the_system_local_timezone() {
        use std::str::FromStr;

        let timestamp = carta_core::Timestamp::from_str("2026-01-01T00:30:00+14:00").unwrap();
        let expected = timestamp.as_datetime().with_timezone(&Local);

        assert_eq!(
            date_only(timestamp),
            expected.format("%Y-%m-%d").to_string()
        );
        assert_eq!(local_timestamp(timestamp), expected.to_rfc3339());
    }

    #[test]
    fn work_status_uses_persisted_color_with_contrasting_text() {
        let (_temporary, mut app) = app_with_documents(&["text"], true);
        let View::Work(work) = &app.view else {
            panic!("expected Work View")
        };
        let work = *work;
        app.archive
            .set_work_color(work, Some("#B5B9A4".into()))
            .unwrap();
        let light = status_style(&app);
        assert_eq!(light.bg, Some(Color::Rgb(181, 185, 164)));
        assert_eq!(light.fg, Some(Color::Black));

        app.archive
            .set_work_color(work, Some("#667A75".into()))
            .unwrap();
        let dark = status_style(&app);
        assert_eq!(dark.bg, Some(Color::Rgb(102, 122, 117)));
        assert_eq!(dark.fg, Some(Color::White));
    }

    #[test]
    fn backspace_is_always_backward_while_delete_uses_cat_erase_direction() {
        let (_temporary, mut app) = app_with_documents(&["abcd"], false);

        app.editor.set_cursor(Cursor { region: 0, byte: 2 }, false);
        app.cat_navigation();
        dispatch(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(app.editor.current_text(), Some("acd"));
        assert_eq!(app.editor.cursor(), Cursor { region: 0, byte: 1 });

        app.editor.set_cursor(Cursor { region: 0, byte: 1 }, false);
        app.cat_navigation();
        dispatch(&mut app, KeyCode::Delete, KeyModifiers::NONE);
        assert_eq!(app.editor.current_text(), Some("ad"));
        assert_eq!(app.editor.cursor(), Cursor { region: 0, byte: 1 });
    }

    #[test]
    fn backspace_erases_an_extended_cat_highlight_as_a_block() {
        let (_temporary, mut app) = app_with_documents(&["alpha beta"], false);
        assert!(app.editor.set_cat_highlight(
            Cursor { region: 0, byte: 6 },
            Cursor {
                region: 0,
                byte: 10
            },
        ));

        dispatch(&mut app, KeyCode::Backspace, KeyModifiers::NONE);

        assert_eq!(app.editor.current_text(), Some("alpha "));
        assert!(app.editor.cat_highlight().is_none());
    }

    #[test]
    fn shift_arrows_move_without_creating_selection() {
        let (_temporary, mut app) = app_with_documents(&["abc\ndef\nghi"], false);
        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        dispatch(&mut app, KeyCode::Right, KeyModifiers::SHIFT);
        dispatch(&mut app, KeyCode::Right, KeyModifiers::SHIFT);
        assert_eq!(app.editor.cursor(), Cursor { region: 0, byte: 2 });
        assert!(app.editor.selection().is_none());

        dispatch(&mut app, KeyCode::Down, KeyModifiers::SHIFT);
        assert!(app.editor.selection().is_none());
    }

    #[test]
    fn leap_enter_builds_a_normal_incremental_pattern() {
        let (_temporary, mut app) = app_with_documents(&["a\nb\n\nc"], false);
        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        app.cat_navigation();
        let original = app.editor.current_text().unwrap().to_owned();
        let mut dispatcher = Dispatcher::default();

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftAlt),
                KeyModifiers::ALT,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT),
            true,
        )
        .unwrap();
        let AppMode::Leap { session, .. } = &app.mode else {
            panic!("expected active LEAP")
        };
        assert_eq!(session.query(), "\n");
        assert_eq!(app.editor.cursor(), Cursor { region: 0, byte: 1 });

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT),
            true,
        )
        .unwrap();
        let AppMode::Leap { session, .. } = &app.mode else {
            panic!("expected active LEAP")
        };
        assert_eq!(session.query(), "\n\n");
        assert_eq!(app.editor.cursor(), Cursor { region: 0, byte: 3 });

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::ALT),
            true,
        )
        .unwrap();
        let AppMode::Leap { session, .. } = &app.mode else {
            panic!("expected active LEAP")
        };
        assert_eq!(session.query(), "\n");
        assert_eq!(app.editor.cursor(), Cursor { region: 0, byte: 1 });

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::ALT),
            true,
        )
        .unwrap();
        let AppMode::Leap { session, .. } = &app.mode else {
            panic!("expected active LEAP")
        };
        assert_eq!(session.query(), "\nc");
        assert_eq!(app.editor.cursor(), Cursor { region: 0, byte: 4 });
        assert_eq!(app.editor.current_text(), Some(original.as_str()));

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new_with_kind(
                KeyCode::Modifier(ModifierKeyCode::LeftAlt),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            ),
            true,
        )
        .unwrap();

        assert!(matches!(app.mode, AppMode::Editing));
        assert_eq!(app.leap.remembered_query(), Some("\nc"));
        assert_eq!(app.editor.current_text(), Some(original.as_str()));
    }

    #[test]
    fn leap_enter_crosses_documents_and_wraps_without_synthetic_lf() {
        let (_temporary, mut app) =
            app_with_documents(&["first", "second\nline", "third"], true);
        let original: Vec<_> = app
            .editor
            .regions()
            .iter()
            .map(|region| region.text.clone())
            .collect();
        let mut dispatcher = Dispatcher::default();

        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        app.cat_navigation();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftAlt),
                KeyModifiers::ALT,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT),
            true,
        )
        .unwrap();
        assert_eq!(app.editor.cursor(), Cursor { region: 1, byte: 6 });
        let AppMode::Leap { session, .. } = &app.mode else {
            panic!("expected active LEAP")
        };
        assert!(!session.current_match().unwrap().wrapped());

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new_with_kind(
                KeyCode::Modifier(ModifierKeyCode::LeftAlt),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            ),
            true,
        )
        .unwrap();

        app.editor.set_cursor(Cursor { region: 2, byte: 5 }, false);
        app.cat_navigation();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftAlt),
                KeyModifiers::ALT,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT),
            true,
        )
        .unwrap();
        assert_eq!(app.editor.cursor(), Cursor { region: 1, byte: 6 });
        let AppMode::Leap { session, .. } = &app.mode else {
            panic!("expected active LEAP")
        };
        assert!(session.current_match().unwrap().wrapped());

        assert_eq!(
            app.editor
                .regions()
                .iter()
                .map(|region| region.text.clone())
                .collect::<Vec<_>>(),
            original
        );
    }

    #[test]
    fn leap_enter_moves_existing_highlight_only_on_release() {
        let (_temporary, mut app) = app_with_documents(&["one\ntwo\nthree"], false);
        app.editor.set_cursor(Cursor { region: 0, byte: 4 }, false);
        assert!(app
            .editor
            .set_cat_highlight(Cursor { region: 0, byte: 4 }, Cursor { region: 0, byte: 7 }));
        let mut dispatcher = Dispatcher::default();

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftControl),
                KeyModifiers::CONTROL,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            true,
        )
        .unwrap();

        assert_eq!(app.editor.cursor(), Cursor { region: 0, byte: 3 });
        assert_eq!(app.editor.current_text(), Some("one\ntwo\nthree"));
        assert_eq!(app.editor.selected_text().as_deref(), Some("two"));

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new_with_kind(
                KeyCode::Modifier(ModifierKeyCode::LeftControl),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            ),
            true,
        )
        .unwrap();

        assert_eq!(app.editor.current_text(), Some("onetwo\n\nthree"));
        assert_eq!(app.editor.selected_text().as_deref(), Some("two"));
    }

    #[test]
    fn active_enter_leap_again_preserves_origin_and_highlights_target_lf() {
        let (_temporary, mut app) = app_with_documents(&["a\nb\nc\nd"], false);
        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        app.cat_navigation();
        let mut dispatcher = Dispatcher::default();

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftAlt),
                KeyModifiers::ALT,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT),
            true,
        )
        .unwrap();
        assert_eq!(app.editor.cursor(), Cursor { region: 0, byte: 1 });

        for expected in [3, 5] {
            handle_key(
                &mut app,
                &mut dispatcher,
                KeyEvent::new(
                    KeyCode::Modifier(ModifierKeyCode::RightControl),
                    KeyModifiers::CONTROL | KeyModifiers::ALT,
                ),
                true,
            )
            .unwrap();

            let AppMode::Leap { session, .. } = &app.mode else {
                panic!("expected active LEAP")
            };
            assert_eq!(session.origin().byte_offset(), 0);
            assert_eq!(app.editor.cursor(), Cursor { region: 0, byte: expected });

            handle_key(
                &mut app,
                &mut dispatcher,
                KeyEvent::new_with_kind(
                    KeyCode::Modifier(ModifierKeyCode::RightControl),
                    KeyModifiers::ALT,
                    KeyEventKind::Release,
                ),
                true,
            )
            .unwrap();
        }

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftControl),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ),
            true,
        )
        .unwrap();

        assert_eq!(app.editor.selected_text().as_deref(), Some("a\nb\nc\n"));
        assert_eq!(dispatcher.clipboard.get_text().unwrap(), "a\nb\nc\n");
    }

    #[test]
    fn enter_leap_highlight_does_not_cross_document_boundary() {
        let (_temporary, mut app) = app_with_documents(&["a", "b\nc"], true);
        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        app.cat_navigation();
        let mut dispatcher = Dispatcher::default();

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftAlt),
                KeyModifiers::ALT,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT),
            true,
        )
        .unwrap();
        assert_eq!(app.editor.cursor(), Cursor { region: 1, byte: 1 });

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftControl),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ),
            true,
        )
        .unwrap();

        assert!(app.editor.cat_highlight().is_none());
        assert_eq!(
            app.status,
            "Cat highlight cannot cross a Document boundary"
        );
    }

    #[test]
    fn released_document_boundary_leap_again_repeats_document_boundary() {
        let (_temporary, mut app) = app_with_documents(&["abc", "def", "ghi"], true);
        app.editor.set_cursor(Cursor { region: 1, byte: 1 }, false);
        app.cat_navigation();
        let mut dispatcher = Dispatcher::default();

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftControl),
                KeyModifiers::CONTROL,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(KeyCode::Home, KeyModifiers::CONTROL),
            true,
        )
        .unwrap();
        assert_eq!(app.editor.cursor(), Cursor { region: 1, byte: 0 });
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new_with_kind(
                KeyCode::Modifier(ModifierKeyCode::LeftControl),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            ),
            true,
        )
        .unwrap();

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::RightControl),
                KeyModifiers::CONTROL,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftControl),
                KeyModifiers::CONTROL,
            ),
            true,
        )
        .unwrap();

        assert_eq!(app.editor.cursor(), Cursor { region: 0, byte: 0 });
    }

    #[test]
    fn view_boundary_leap_again_does_not_fall_back_to_old_text_query() {
        let (_temporary, mut app) = app_with_documents(&["target", "middle", "target"], true);
        app.start_leap(LeapDirection::Forward, true);
        app.leap_input("target");
        app.end_leap();
        assert_eq!(app.leap.remembered_query(), Some("target"));

        app.editor.set_cursor(Cursor { region: 1, byte: 1 }, false);
        app.cat_navigation();
        app.leap_view_boundary(LeapDirection::Forward);
        let at_end = app.editor.cursor();

        app.leap_again(LeapDirection::Forward);

        assert_eq!(app.editor.cursor(), at_end);
    }

    #[test]
    fn cat_boundary_leaps_target_document_and_view_edges() {
        let (_temporary, mut app) = app_with_documents(&["abc", "def", "ghi"], true);

        let cases = [
            (
                Cursor { region: 1, byte: 1 },
                ModifierKeyCode::LeftControl,
                KeyModifiers::CONTROL,
                KeyCode::Home,
                Cursor { region: 1, byte: 0 },
            ),
            (
                Cursor { region: 1, byte: 1 },
                ModifierKeyCode::LeftAlt,
                KeyModifiers::ALT,
                KeyCode::End,
                Cursor { region: 1, byte: 3 },
            ),
            (
                Cursor { region: 1, byte: 1 },
                ModifierKeyCode::LeftControl,
                KeyModifiers::CONTROL,
                KeyCode::PageUp,
                Cursor { region: 0, byte: 0 },
            ),
            (
                Cursor { region: 1, byte: 1 },
                ModifierKeyCode::LeftAlt,
                KeyModifiers::ALT,
                KeyCode::PageDown,
                Cursor { region: 2, byte: 3 },
            ),
        ];

        for (origin, modifier, modifiers, code, expected) in cases {
            app.editor.set_cursor(origin, false);
            app.cat_navigation();
            let mut dispatcher = Dispatcher::default();
            handle_key(
                &mut app,
                &mut dispatcher,
                KeyEvent::new(KeyCode::Modifier(modifier), modifiers),
                true,
            )
            .unwrap();
            handle_key(
                &mut app,
                &mut dispatcher,
                KeyEvent::new(code, modifiers),
                true,
            )
            .unwrap();
            assert_eq!(app.editor.cursor(), expected);
        }
    }

    #[test]
    fn both_leap_keys_extend_the_last_leap_into_cat_highlight() {
        let (_temporary, mut app) = app_with_documents(&["alpha beta gamma"], false);
        let mut dispatcher = Dispatcher::default();

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftAlt),
                KeyModifiers::ALT,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(KeyCode::Char('b'), KeyModifiers::ALT),
            true,
        )
        .unwrap();
        let release = KeyEvent::new_with_kind(
            KeyCode::Modifier(ModifierKeyCode::LeftAlt),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        );
        handle_key(&mut app, &mut dispatcher, release, true).unwrap();

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftControl),
                KeyModifiers::CONTROL,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftAlt),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ),
            true,
        )
        .unwrap();

        assert!(app.editor.cat_highlight().is_some());
        let selected = app.editor.selected_text().expect("Cat highlight text");
        assert_eq!(dispatcher.clipboard.get_text().unwrap(), selected);
    }

    #[test]
    fn right_control_with_leap_keys_performs_leap_again() {
        let (_temporary, mut app) = app_with_documents(&["one one one"], false);
        let mut dispatcher = Dispatcher::default();

        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        app.start_leap(LeapDirection::Forward, true);
        app.leap_input("one");
        app.end_leap();
        assert_eq!(app.editor.cursor().byte, 0);

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::RightControl),
                KeyModifiers::CONTROL,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftAlt),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ),
            true,
        )
        .unwrap();
        assert_eq!(app.editor.cursor().byte, 4);

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new_with_kind(
                KeyCode::Modifier(ModifierKeyCode::LeftAlt),
                KeyModifiers::CONTROL,
                KeyEventKind::Release,
            ),
            true,
        )
        .unwrap();
        assert_eq!(app.editor.cursor().byte, 4);

        app.editor.set_cursor(Cursor { region: 0, byte: 8 }, false);
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftControl),
                KeyModifiers::CONTROL,
            ),
            true,
        )
        .unwrap();
        assert_eq!(app.editor.cursor().byte, 4);
    }

    #[test]
    fn right_control_z_and_r_dispatch_undo_and_redo() {
        let (_temporary, mut app) = app_with_documents(&["a"], false);
        app.editor.set_cursor(Cursor { region: 0, byte: 1 }, false);
        assert!(app.cat_insert("b"));
        assert_eq!(app.editor.current_text(), Some("ab"));

        let mut dispatcher = Dispatcher::default();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::RightControl),
                KeyModifiers::CONTROL,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL),
            true,
        )
        .unwrap();
        assert_eq!(app.editor.current_text(), Some("a"));

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
            true,
        )
        .unwrap();
        assert_eq!(app.editor.current_text(), Some("ab"));
    }

    #[test]
    fn right_control_c_copies_cat_highlight() {
        let (_temporary, mut app) = app_with_documents(&["alpha beta"], false);
        assert!(app.editor.set_cat_highlight(
            Cursor { region: 0, byte: 6 },
            Cursor {
                region: 0,
                byte: 10
            },
        ));
        let mut dispatcher = Dispatcher::default();

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::RightControl),
                KeyModifiers::CONTROL,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            true,
        )
        .unwrap();

        assert_eq!(app.editor.regions()[0].text, "alpha betabeta");
        assert_eq!(app.editor.selected_text().as_deref(), Some("beta"));
    }

    #[test]
    fn right_control_c_without_highlight_pastes_system_clipboard() {
        let (_temporary, mut app) = app_with_documents(&["alpha beta"], false);
        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        let mut dispatcher = Dispatcher::default();
        dispatcher
            .clipboard
            .set_text("outside\r\ntext".into())
            .unwrap();

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::RightControl),
                KeyModifiers::CONTROL,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            true,
        )
        .unwrap();

        assert_eq!(app.editor.regions()[0].text, "outside\ntextalpha beta");
        assert!(app.editor.cat_highlight().is_none());
    }

    #[test]
    fn leap_moves_cat_highlight_to_another_document_on_release() {
        let (_temporary, mut app) = app_with_documents(&["alpha beta", "target"], false);
        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        assert!(app.editor.set_cat_highlight(
            Cursor { region: 0, byte: 6 },
            Cursor {
                region: 0,
                byte: 10
            },
        ));

        app.start_leap(LeapDirection::Forward, false);
        app.leap_input("target");
        app.end_leap();

        assert_eq!(app.editor.regions()[0].text, "alpha ");
        assert_eq!(app.editor.regions()[1].text, "betatarget");
        assert_eq!(app.editor.selected_text().as_deref(), Some("beta"));
    }

    #[test]
    fn ctrl_home_and_end_stay_within_current_document() {
        let (_temporary, mut app) = app_with_documents(&["first", "second"], false);
        app.editor.set_cursor(Cursor { region: 1, byte: 3 }, false);
        dispatch(&mut app, KeyCode::Home, KeyModifiers::CONTROL);
        assert_eq!(app.editor.cursor(), Cursor { region: 1, byte: 0 });
        dispatch(&mut app, KeyCode::End, KeyModifiers::CONTROL);
        assert_eq!(app.editor.cursor(), Cursor { region: 1, byte: 6 });
    }

    #[test]
    fn ctrl_page_moves_between_documents_without_wrap_or_content_changes() {
        for work in [false, true] {
            let (_temporary, mut app) = app_with_documents(&["first", "second", "third"], work);
            let original_view = app.view.clone();
            let original: Vec<_> = app
                .editor
                .regions()
                .iter()
                .map(|region| region.text.clone())
                .collect();

            dispatch(&mut app, KeyCode::PageUp, KeyModifiers::CONTROL);
            assert_eq!(app.editor.cursor(), Cursor { region: 0, byte: 0 });
            dispatch(&mut app, KeyCode::PageDown, KeyModifiers::CONTROL);
            assert_eq!(app.editor.cursor(), Cursor { region: 1, byte: 0 });
            dispatch(&mut app, KeyCode::PageDown, KeyModifiers::CONTROL);
            dispatch(&mut app, KeyCode::PageDown, KeyModifiers::CONTROL);
            assert_eq!(app.editor.cursor(), Cursor { region: 2, byte: 0 });
            dispatch(&mut app, KeyCode::PageUp, KeyModifiers::CONTROL);
            assert_eq!(app.editor.cursor(), Cursor { region: 1, byte: 0 });
            assert_eq!(
                app.editor
                    .regions()
                    .iter()
                    .map(|region| region.text.clone())
                    .collect::<Vec<_>>(),
                original
            );
            assert_eq!(app.view, original_view);
        }
    }

    #[test]
    fn assigned_right_control_chords_do_not_change_remembered_leap_query() {
        let (_temporary, mut app) = app_with_documents(&["alpha beta"], false);
        app.start_leap(LeapDirection::Forward, true);
        app.leap_input("alpha");
        app.end_leap();
        assert_eq!(app.leap.remembered_query(), Some("alpha"));

        for code in [
            KeyCode::PageUp,
            KeyCode::PageDown,
            KeyCode::Home,
            KeyCode::End,
        ] {
            let mut dispatcher = Dispatcher {
                right_control_held: true,
                ..Dispatcher::default()
            };
            handle_key(
                &mut app,
                &mut dispatcher,
                KeyEvent::new(code, KeyModifiers::CONTROL),
                true,
            )
            .unwrap();

            assert!(dispatcher.pending_leap.is_none());
            assert!(matches!(app.mode, AppMode::Editing));
            assert_eq!(app.leap.remembered_query(), Some("alpha"));
        }
    }

    #[test]
    fn degraded_control_chords_do_not_change_remembered_leap_query() {
        let (_temporary, mut app) = app_with_documents(&["alpha beta"], false);
        app.start_leap(LeapDirection::Forward, true);
        app.leap_input("alpha");
        app.end_leap();

        for code in [
            KeyCode::PageUp,
            KeyCode::PageDown,
            KeyCode::Home,
            KeyCode::End,
        ] {
            let mut dispatcher = Dispatcher::default();
            handle_key(
                &mut app,
                &mut dispatcher,
                KeyEvent::new(code, KeyModifiers::CONTROL),
                false,
            )
            .unwrap();

            assert!(matches!(app.mode, AppMode::Editing));
            assert_eq!(app.leap.remembered_query(), Some("alpha"));
        }
    }

    #[test]
    fn shift_pressed_after_leap_key_remains_part_of_the_leap_pattern() {
        for (key, modifiers, direction, origin, expected) in [
            (
                ModifierKeyCode::LeftAlt,
                KeyModifiers::ALT,
                LeapDirection::Forward,
                0,
                1,
            ),
            (
                ModifierKeyCode::LeftControl,
                KeyModifiers::CONTROL,
                LeapDirection::Backward,
                3,
                1,
            ),
        ] {
            let (_temporary, mut app) = app_with_documents(&["a#b"], false);
            app.editor.set_cursor(
                Cursor {
                    region: 0,
                    byte: origin,
                },
                false,
            );
            app.cat_navigation();
            let mut dispatcher = Dispatcher::default();

            handle_key(
                &mut app,
                &mut dispatcher,
                KeyEvent::new(KeyCode::Modifier(key), modifiers),
                true,
            )
            .unwrap();
            assert!(dispatcher.pending_leap.is_some());

            handle_key(
                &mut app,
                &mut dispatcher,
                KeyEvent::new(
                    KeyCode::Modifier(ModifierKeyCode::LeftShift),
                    modifiers | KeyModifiers::SHIFT,
                ),
                true,
            )
            .unwrap();
            assert!(dispatcher.pending_leap.is_some());

            handle_key(
                &mut app,
                &mut dispatcher,
                KeyEvent::new(KeyCode::Char('#'), modifiers | KeyModifiers::SHIFT),
                true,
            )
            .unwrap();
            assert!(matches!(app.mode, AppMode::Leap { .. }));

            handle_key(
                &mut app,
                &mut dispatcher,
                KeyEvent::new_with_kind(
                    KeyCode::Modifier(key),
                    KeyModifiers::SHIFT,
                    KeyEventKind::Release,
                ),
                true,
            )
            .unwrap();

            assert_eq!(app.editor.cursor().byte, expected);
            assert_eq!(app.leap.remembered_query(), Some("#"));
            assert!(matches!(app.mode, AppMode::Editing));
            assert_eq!(
                direction,
                if key == ModifierKeyCode::LeftAlt {
                    LeapDirection::Forward
                } else {
                    LeapDirection::Backward
                }
            );
        }
    }

    #[test]
    fn altgr_pressed_after_leap_key_does_not_cancel_pending_leap() {
        let (_temporary, mut app) = app_with_documents(&["a@b"], false);
        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        app.cat_navigation();
        let mut dispatcher = Dispatcher::default();

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftAlt),
                KeyModifiers::ALT,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::RightAlt),
                KeyModifiers::ALT,
            ),
            true,
        )
        .unwrap();
        assert!(dispatcher.pending_leap.is_some());

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(KeyCode::Char('@'), KeyModifiers::ALT),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new_with_kind(
                KeyCode::Modifier(ModifierKeyCode::LeftAlt),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            ),
            true,
        )
        .unwrap();

        assert_eq!(app.editor.cursor().byte, 1);
        assert_eq!(app.leap.remembered_query(), Some("@"));
    }

    #[test]
    fn tapping_physical_leap_keys_creeps_one_character() {
        let (_temporary, mut app) = app_with_documents(&["abcd"], false);
        app.editor.set_cursor(Cursor { region: 0, byte: 1 }, false);
        app.cat_navigation();
        let mut dispatcher = Dispatcher::default();

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftAlt),
                KeyModifiers::ALT,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new_with_kind(
                KeyCode::Modifier(ModifierKeyCode::LeftAlt),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            ),
            true,
        )
        .unwrap();
        assert_eq!(app.editor.cursor(), Cursor { region: 0, byte: 2 });

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftControl),
                KeyModifiers::CONTROL,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new_with_kind(
                KeyCode::Modifier(ModifierKeyCode::LeftControl),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            ),
            true,
        )
        .unwrap();
        assert_eq!(app.editor.cursor(), Cursor { region: 0, byte: 1 });
    }

    #[test]
    fn shift_modified_pending_control_still_returns_to_normal_input_after_release() {
        let (_temporary, mut app) = app_with_documents(&["alpha beta"], false);
        app.start_leap(LeapDirection::Forward, true);
        app.leap_input("beta");
        app.end_leap();
        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        let mut dispatcher = Dispatcher::default();

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftControl),
                KeyModifiers::CONTROL,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftShift),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new_with_kind(
                KeyCode::Modifier(ModifierKeyCode::LeftControl),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE),
            true,
        )
        .unwrap();

        assert_eq!(app.editor.regions()[0].text, "palpha beta");
        assert!(matches!(app.mode, AppMode::Editing));
    }

    #[test]
    fn opposite_leap_can_highlight_while_use_front_is_still_held() {
        let (_temporary, mut app) = app_with_documents(&["a x b x c"], false);
        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        app.cat_navigation();
        let mut dispatcher = Dispatcher::default();

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftAlt),
                KeyModifiers::ALT,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT),
            true,
        )
        .unwrap();

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::RightControl),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ),
            true,
        )
        .unwrap();
        assert_eq!(app.editor.cursor().byte, 6);
        assert!(dispatcher.right_control_held);

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftControl),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ),
            true,
        )
        .unwrap();

        assert!(matches!(app.mode, AppMode::Editing));
        assert_eq!(app.editor.selected_text().as_deref(), Some("a x b x"));
    }

    #[test]
    fn active_leap_again_keeps_original_anchor_until_opposite_leap_highlights() {
        let (_temporary, mut app) = app_with_documents(&["a x b x c x d"], false);
        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        app.cat_navigation();
        let mut dispatcher = Dispatcher::default();

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftAlt),
                KeyModifiers::ALT,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT),
            true,
        )
        .unwrap();
        assert_eq!(app.editor.cursor().byte, 2);

        for expected in [6, 10] {
            handle_key(
                &mut app,
                &mut dispatcher,
                KeyEvent::new(
                    KeyCode::Modifier(ModifierKeyCode::RightControl),
                    KeyModifiers::CONTROL | KeyModifiers::ALT,
                ),
                true,
            )
            .unwrap();

            let AppMode::Leap { session, .. } = &app.mode else {
                panic!("expected active LEAP")
            };
            assert_eq!(session.origin().byte_offset(), 0);
            assert_eq!(app.editor.cursor().byte, expected);

            handle_key(
                &mut app,
                &mut dispatcher,
                KeyEvent::new_with_kind(
                    KeyCode::Modifier(ModifierKeyCode::RightControl),
                    KeyModifiers::ALT,
                    KeyEventKind::Release,
                ),
                true,
            )
            .unwrap();
        }

        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftControl),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ),
            true,
        )
        .unwrap();

        assert!(matches!(app.mode, AppMode::Editing));
        assert!(dispatcher.active_leap.is_none());
        assert_eq!(app.editor.selected_text().as_deref(), Some("a x b x c x"));
        assert_eq!(dispatcher.clipboard.get_text().unwrap(), "a x b x c x");

        for released in [ModifierKeyCode::LeftControl, ModifierKeyCode::LeftAlt] {
            handle_key(
                &mut app,
                &mut dispatcher,
                KeyEvent::new_with_kind(
                    KeyCode::Modifier(released),
                    KeyModifiers::NONE,
                    KeyEventKind::Release,
                ),
                true,
            )
            .unwrap();
        }
        assert!(app.editor.cat_highlight().is_some());
    }

    #[test]
    fn left_control_and_left_alt_keep_leap_behavior() {
        for (key, modifiers, direction) in [
            (
                ModifierKeyCode::LeftControl,
                KeyModifiers::CONTROL,
                LeapDirection::Backward,
            ),
            (
                ModifierKeyCode::LeftAlt,
                KeyModifiers::ALT,
                LeapDirection::Forward,
            ),
        ] {
            let (_temporary, mut app) = app_with_documents(&["alpha beta alpha"], false);
            app.editor.set_cursor(Cursor { region: 0, byte: 6 }, false);
            let mut dispatcher = Dispatcher::default();
            handle_key(
                &mut app,
                &mut dispatcher,
                KeyEvent::new(KeyCode::Modifier(key), modifiers),
                true,
            )
            .unwrap();
            for character in "alpha".chars() {
                handle_key(
                    &mut app,
                    &mut dispatcher,
                    KeyEvent::new(KeyCode::Char(character), modifiers),
                    true,
                )
                .unwrap();
            }
            handle_key(
                &mut app,
                &mut dispatcher,
                KeyEvent::new_with_kind(
                    KeyCode::Modifier(key),
                    KeyModifiers::NONE,
                    KeyEventKind::Release,
                ),
                true,
            )
            .unwrap();
            assert_eq!(app.leap.remembered_query(), Some("alpha"));
            assert!(matches!(app.mode, AppMode::Editing));
            assert_eq!(
                app.editor.cursor().byte,
                match direction {
                    LeapDirection::Backward => 0,
                    LeapDirection::Forward => 11,
                }
            );
        }
    }

    #[test]
    fn right_modifiers_are_not_leap_and_altgr_text_is_inserted() {
        let (_temporary, mut app) = app_with_documents(&[""], false);
        let mut dispatcher = Dispatcher::default();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::RightControl),
                KeyModifiers::CONTROL,
            ),
            true,
        )
        .unwrap();
        assert!(dispatcher.pending_leap.is_none());
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::RightAlt),
                KeyModifiers::ALT,
            ),
            true,
        )
        .unwrap();
        assert!(dispatcher.pending_leap.is_none());
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(KeyCode::Char('@'), KeyModifiers::ALT),
            true,
        )
        .unwrap();
        assert_eq!(app.editor.current_text(), Some("@"));
    }

    #[test]
    fn leap_enter_moves_to_visual_line_edge_and_release_is_inert() {
        let (_temporary, mut app) = app_with_documents(&["abcdefghij"], false);
        app.editor.set_cursor(Cursor { region: 0, byte: 7 }, false);
        let mut dispatcher = Dispatcher::default();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftControl),
                KeyModifiers::CONTROL,
            ),
            true,
        )
        .unwrap();
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            true,
        )
        .unwrap();
        assert_eq!(app.editor.cursor().byte, 0);
        handle_key(
            &mut app,
            &mut dispatcher,
            KeyEvent::new_with_kind(
                KeyCode::Modifier(ModifierKeyCode::LeftControl),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            ),
            true,
        )
        .unwrap();
        assert!(matches!(app.mode, AppMode::Editing));
    }

    #[test]
    fn layout_resolved_text_is_inserted_without_key_mapping() {
        let (_temporary, mut app) = app_with_documents(&[""], false);
        dispatch(&mut app, KeyCode::Char('A'), KeyModifiers::NONE);
        dispatch(&mut app, KeyCode::Char('!'), KeyModifiers::NONE);
        dispatch(&mut app, KeyCode::Char('É'), KeyModifiers::NONE);
        assert_eq!(app.editor.regions()[0].text, "A!É");
    }

    #[test]
    fn specialized_status_lines_include_date_context_and_position() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        let document = app.editor.current_document().unwrap();
        assert!(app.editor.insert("# Label"));
        app.autosave().unwrap();
        app.history = app.archive.document_revisions(document).unwrap();
        app.view = View::History {
            document,
            selected: 0,
        };
        let history = status_line(&app);
        assert!(history.contains("History · Label · 1/1"));
        assert!(history.starts_with(&date_only(app.history[0].checkpoint().created())));

        app.archive.trash_document(document).unwrap();
        app.trash = Some(app.archive.trash_inventory().unwrap());
        app.view = View::Trash { selected: 0 };
        let trash = status_line(&app);
        assert!(trash.contains("Trash · Document: Label · 1/1"));
        assert!(trash.starts_with(&date_only(
            app.trash.as_ref().unwrap().documents()[0].created()
        )));
    }
}
