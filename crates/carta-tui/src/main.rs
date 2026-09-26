use carta_core::{Archive, LeapDirection};
use carta_tui::app::{AppMode, View};
use carta_tui::editor::{visual_ranges, Cursor};
use carta_tui::session::{
    load_last_archive, load_session, save_last_archive, save_session, state_root,
};
use carta_tui::App;
use chrono::{DateTime, Datelike, Weekday};
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
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
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
    let archive = if args.create {
        Archive::create(&path)?
    } else {
        Archive::open(&path)?
    };
    let canonical = archive
        .root()
        .canonicalize()
        .unwrap_or_else(|_| archive.root().to_path_buf());
    save_last_archive(&state, &canonical)?;
    let session = load_session(&state, archive.metadata().archive_id())?;
    let archive_id = archive.metadata().archive_id();
    let mut app = App::open(archive, session.as_ref(), Instant::now())?;
    let mut terminal = TerminalGuard::enter()?;
    if !terminal.enhancements {
        app.status = "Compatibility keyboard mode: use palette LEAP commands".into();
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
                app.start_leap(pending.direction, false);
                app.end_leap();
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
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('c' | 'C'))
    {
        dispatcher.pending_leap = None;
        if matches!(app.mode, AppMode::Editing)
            && matches!(app.view, View::Chronological(_) | View::Work(_))
        {
            app.copy_cat_highlight();
        }
        return Ok(());
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        let editable = matches!(app.mode, AppMode::Editing)
            && matches!(app.view, View::Chronological(_) | View::Work(_));
        match key.code {
            KeyCode::PageUp if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                dispatcher.pending_leap = None;
                if editable {
                    app.editor.move_document(false);
                }
                return Ok(());
            }
            KeyCode::PageDown if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                dispatcher.pending_leap = None;
                if editable {
                    app.editor.move_document(true);
                }
                return Ok(());
            }
            KeyCode::Home if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                dispatcher.pending_leap = None;
                if editable {
                    app.editor.document_home(false);
                }
                return Ok(());
            }
            KeyCode::End if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                dispatcher.pending_leap = None;
                if editable {
                    app.editor.document_end(false);
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
                return Ok(());
            }
            KeyCode::Modifier(key @ (ModifierKeyCode::LeftControl | ModifierKeyCode::LeftAlt)) => {
                if dispatcher.active_leap.is_some() {
                    return Ok(());
                }
                if let Some(pending) = dispatcher.pending_leap {
                    if pending.key != key {
                        dispatcher.pending_leap = None;
                        dispatcher.suppressed_leap_releases = 2;
                        app.extend_last_leap_highlight();
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
        if key.code == KeyCode::Enter {
            let (terminal_width, _) = crossterm::terminal::size().unwrap_or((80, 24));
            let width = editor_width(terminal_width);
            if pending.direction == LeapDirection::Backward {
                app.editor.visual_home(width, false);
            } else {
                app.editor.visual_end(width, false);
            }
            return Ok(());
        }
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
                }
            }
            KeyCode::Esc => app.cancel_leap(),
            _ => {}
        },
        AppMode::Palette { .. } => unreachable!(),
        AppMode::Prompt { input, .. } => match key.code {
            KeyCode::Esc => app.cancel_mode(),
            KeyCode::Enter => app.submit_prompt()?,
            KeyCode::Backspace => {
                input.pop();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => input.push(c),
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
    ) {
        match key.code {
            KeyCode::Up => app.move_list_selection(false),
            KeyCode::Down => app.move_list_selection(true),
            KeyCode::Enter => app.open_selected()?,
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
            app.editor.insert(&c.to_string())
        }
        KeyCode::Enter => app.editor.insert_newline_with_list_continuation(),
        KeyCode::Tab if shift => app.editor.indent_less(),
        KeyCode::Tab => app.editor.insert("    "),
        KeyCode::Backspace => app.editor.backspace(),
        KeyCode::Delete => app.editor.delete(),
        KeyCode::Left => {
            app.editor.move_horizontal(false, false);
            false
        }
        KeyCode::Right => {
            app.editor.move_horizontal(true, false);
            false
        }
        KeyCode::Up => {
            app.editor.move_visual(false, width, false);
            false
        }
        KeyCode::Down => {
            app.editor.move_visual(true, width, false);
            false
        }
        KeyCode::Home => {
            app.editor.visual_home(width, false);
            false
        }
        KeyCode::End => {
            app.editor.visual_end(width, false);
            false
        }
        KeyCode::PageUp => {
            app.editor.page_visual(false, page, width, false);
            false
        }
        KeyCode::PageDown => {
            app.editor.page_visual(true, page, width, false);
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
                        r.checkpoint().created(),
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
                        snapshot.checkpoint().created(),
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
                items.extend(
                    trash.documents().iter().map(|d| {
                        ListItem::new(format!("Document · {} · {}", d.created(), d.label()))
                    }),
                );
                items.extend(
                    trash
                        .works()
                        .iter()
                        .map(|w| ListItem::new(format!("Work · {} · {}", w.created(), w.title()))),
                );
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
    if cursor_line < app.scroll {
        app.scroll = cursor_line;
    } else if cursor_line >= app.scroll + height {
        app.scroll = cursor_line + 1 - height;
    }
    let selection = app.editor.selection();
    let rendered: Vec<_> = lines
        .iter()
        .skip(app.scroll)
        .take(height)
        .map(|line| {
            if let Some(region) = line.region {
                styled_line(line, region, selection)
            } else {
                Line::styled(line.text.clone(), Style::default().fg(Color::DarkGray))
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(rendered), area);
    if matches!(app.mode, AppMode::Editing | AppMode::Leap { .. }) {
        if let Some(line) = lines.get(cursor_line) {
            let text =
                &app.editor.regions()[cursor.region].text[line.start..cursor.byte.min(line.end)];
            let x = text.chars().map(|c| c.width().unwrap_or(0)).sum::<usize>();
            frame.set_cursor_position((
                area.x + x.min(width.saturating_sub(1)) as u16,
                area.y + cursor_line.saturating_sub(app.scroll) as u16,
            ));
        }
    }
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

        for (start, end) in visual_ranges(&region.text, width) {
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
    let raw = timestamp.to_string();
    let Ok(date) = DateTime::parse_from_rfc3339(&raw) else {
        return raw.chars().take(10).collect();
    };
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

fn styled_line(line: &VisualLine, region: usize, selection: Option<(Cursor, Cursor)>) -> Line<'_> {
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
        Span::styled(text[a..b].to_owned(), Style::default().bg(Color::Blue)),
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
                    + input
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
fn date_only(timestamp: carta_core::Timestamp) -> String {
    timestamp.to_string().chars().take(10).collect()
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
        assert!(app.editor.selected_text().is_some());
    }

    #[test]
    fn right_control_c_copies_cat_highlight() {
        let (_temporary, mut app) =
            app_with_documents(&["alpha beta"], false);
        assert!(app.editor.set_cat_highlight(
            Cursor { region: 0, byte: 6 },
            Cursor { region: 0, byte: 10 },
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
    fn copy_after_leap_auto_extends_the_last_leap_span() {
        let (_temporary, mut app) = app_with_documents(&["alpha beta"], false);
        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        app.start_leap(LeapDirection::Forward, false);
        app.leap_input("beta");
        app.end_leap();

        assert!(app.editor.cat_highlight().is_none());
        assert!(app.copy_cat_highlight());
        assert_eq!(app.editor.regions()[0].text, "alpha betaalpha beta");
        assert_eq!(app.editor.selected_text().as_deref(), Some("alpha beta"));
    }

    #[test]
    fn leap_moves_cat_highlight_to_another_document_on_release() {
        let (_temporary, mut app) =
            app_with_documents(&["alpha beta", "target"], false);
        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        assert!(app.editor.set_cat_highlight(
            Cursor { region: 0, byte: 6 },
            Cursor { region: 0, byte: 10 },
        ));

        app.start_leap(LeapDirection::Forward, false);
        app.leap_input("target");
        app.end_leap();

        assert_eq!(app.editor.regions()[0].text, "alpha ");
        assert_eq!(app.editor.regions()[1].text, "targetbeta");
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
    fn assigned_control_chords_do_not_change_remembered_leap_query() {
        let (_temporary, mut app) = app_with_documents(&["alpha beta"], false);
        app.start_leap(LeapDirection::Forward, true);
        app.leap_input("alpha");
        app.end_leap();
        assert_eq!(app.leap.remembered_query(), Some("alpha"));
        let chords = [
            (KeyCode::PageUp, KeyModifiers::CONTROL),
            (KeyCode::PageDown, KeyModifiers::CONTROL),
            (KeyCode::Home, KeyModifiers::CONTROL),
            (KeyCode::End, KeyModifiers::CONTROL),
        ];
        for (code, modifiers) in chords {
            let mut dispatcher = Dispatcher {
                pending_leap: Some(PendingLeap {
                    direction: LeapDirection::Backward,
                    key: ModifierKeyCode::LeftControl,
                }),
                ..Dispatcher::default()
            };
            handle_key(
                &mut app,
                &mut dispatcher,
                KeyEvent::new(code, modifiers),
                true,
            )
            .unwrap();
            assert!(dispatcher.pending_leap.is_none());
            assert!(matches!(app.mode, AppMode::Editing));
            assert_eq!(app.leap.remembered_query(), Some("alpha"));
        }
    }

    #[test]
    fn unrelated_modifier_cancels_pending_leap_without_starting_selection() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        assert!(app.editor.insert("selected"));
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
        assert!(matches!(app.mode, AppMode::Editing));
        assert!(app.editor.selection().is_none());
        assert!(dispatcher.pending_leap.is_none());
    }

    #[test]
    fn cancelled_control_does_not_leap_again_and_pasted_text_stays_normal_input() {
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
                    LeapDirection::Forward => 16,
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
