use carta_core::{Archive, LeapDirection};
use carta_tui::app::{AppMode, View};
use carta_tui::clipboard::Clipboard;
use carta_tui::editor::Cursor;
use carta_tui::session::{
    load_last_archive, load_session, save_last_archive, save_session, state_root,
};
use carta_tui::App;
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
use std::error::Error;
use std::io::{self, Stdout};
use std::path::PathBuf;
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
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, Hide)?;
        let enhancements = supports_keyboard_enhancement().unwrap_or(false);
        if enhancements {
            let flags = KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES;
            execute!(stdout, PushKeyboardEnhancementFlags(flags))?;
        }
        Ok(Self {
            terminal: Terminal::new(CrosstermBackend::new(stdout))?,
            enhancements,
        })
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
    if let Err(error) = run() {
        eprintln!("carta-tui: {error}");
        std::process::exit(1);
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
    let mut clipboard = Clipboard::default();
    let mut dispatcher = Dispatcher::default();
    let mut last_session_save = Instant::now();

    while !app.quit {
        terminal.terminal.draw(|frame| draw(frame, &mut app))?;
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if let Err(error) = handle_key(
                    &mut app,
                    &mut clipboard,
                    &mut dispatcher,
                    key,
                    terminal.enhancements,
                ) {
                    app.status = error.to_string();
                    app.mode = AppMode::Editing;
                    let _ = app.reveal_conflicts();
                }
                if app.take_wiped_document().is_some() {
                    clipboard.clear_internal();
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
    pending_control: Option<LeapDirection>,
}

fn handle_key(
    app: &mut App,
    clipboard: &mut Clipboard,
    dispatcher: &mut Dispatcher,
    key: KeyEvent,
    enhanced: bool,
) -> Result<(), Box<dyn Error>> {
    if key.kind == KeyEventKind::Release {
        if matches!(
            key.code,
            KeyCode::Modifier(ModifierKeyCode::LeftControl | ModifierKeyCode::RightControl)
        ) {
            if let Some(direction) = dispatcher.pending_control.take() {
                app.start_leap(direction, false);
                app.end_leap();
            } else if matches!(app.mode, AppMode::Leap { .. }) {
                app.end_leap();
            }
        }
        return Ok(());
    }
    if key.kind != KeyEventKind::Press && key.kind != KeyEventKind::Repeat {
        return Ok(());
    }

    if key
        .modifiers
        .contains(KeyModifiers::CONTROL | KeyModifiers::SHIFT)
    {
        match key.code {
            KeyCode::Char('c' | 'C') => {
                dispatcher.pending_control = None;
                if let Some(text) = app.editor.selected_text() {
                    clipboard.copy(text);
                }
                return Ok(());
            }
            KeyCode::Char('x' | 'X') => {
                dispatcher.pending_control = None;
                if let Some(text) = app.editor.selected_text() {
                    if app.editor.delete_selection() {
                        clipboard.copy(text);
                        app.edited(Instant::now());
                    } else {
                        app.status = "Cut cannot cross a Document boundary".into();
                    }
                }
                return Ok(());
            }
            KeyCode::Char('v' | 'V') => {
                dispatcher.pending_control = None;
                if !app.editor.insert(&clipboard.paste()) {
                    app.status = "Paste cannot replace a cross-boundary selection".into();
                } else {
                    app.edited(Instant::now());
                }
                return Ok(());
            }
            KeyCode::Char('z' | 'Z') => {
                dispatcher.pending_control = None;
                if app.editor.undo() {
                    app.edited(Instant::now());
                }
                return Ok(());
            }
            KeyCode::Char('y' | 'Y') => {
                dispatcher.pending_control = None;
                if app.editor.redo() {
                    app.edited(Instant::now());
                }
                return Ok(());
            }
            _ => {}
        }
    }

    if enhanced && key.kind == KeyEventKind::Press {
        match key.code {
            KeyCode::Modifier(ModifierKeyCode::LeftControl) => {
                dispatcher.pending_control = Some(LeapDirection::Backward);
                return Ok(());
            }
            KeyCode::Modifier(ModifierKeyCode::RightControl) => {
                dispatcher.pending_control = Some(LeapDirection::Forward);
                return Ok(());
            }
            _ => {}
        }
    }

    if dispatcher.pending_control.is_some() && matches!(key.code, KeyCode::Modifier(_)) {
        return Ok(());
    }
    if let Some(direction) = dispatcher.pending_control.take() {
        app.start_leap(direction, false);
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
    let (width, height) = crossterm::terminal::size().unwrap_or((80, 24));
    let width = usize::from(width.max(1));
    let page = usize::from(height.saturating_sub(2).max(1));
    let changed = match key.code {
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.editor.insert(&c.to_string())
        }
        KeyCode::Enter => app.editor.insert("\n"),
        KeyCode::Tab if shift => app.editor.indent_less(),
        KeyCode::Tab => app.editor.insert("    "),
        KeyCode::Backspace => app.editor.backspace(),
        KeyCode::Delete => app.editor.delete(),
        KeyCode::Left => {
            app.editor.move_horizontal(false, shift);
            false
        }
        KeyCode::Right => {
            app.editor.move_horizontal(true, shift);
            false
        }
        KeyCode::Up => {
            app.editor.move_visual(false, width, shift);
            false
        }
        KeyCode::Down => {
            app.editor.move_visual(true, width, shift);
            false
        }
        KeyCode::Home => {
            app.editor.visual_home(width, shift);
            false
        }
        KeyCode::End => {
            app.editor.visual_end(width, shift);
            false
        }
        KeyCode::PageUp => {
            app.editor.page_visual(false, page, width, shift);
            false
        }
        KeyCode::PageDown => {
            app.editor.page_visual(true, page, width, shift);
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
        View::Chronological(_) | View::Work(_) => draw_editor(frame, app, chunks[0]),
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
        Paragraph::new(status_line(app))
            .style(Style::default().bg(Color::DarkGray).fg(Color::White)),
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

fn draw_editor(frame: &mut ratatui::Frame<'_>, app: &mut App, area: Rect) {
    let width = usize::from(area.width.max(1));
    let lines = visual_lines(&app.editor, width);
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

fn visual_lines(editor: &carta_tui::CompositeEditor, width: usize) -> Vec<VisualLine> {
    let mut out = Vec::new();
    for (region_index, region) in editor.regions().iter().enumerate() {
        if region_index > 0 {
            out.push(VisualLine {
                region: None,
                start: 0,
                end: 0,
                text: "─".repeat(width),
            });
        }
        if region.text.is_empty() {
            out.push(VisualLine {
                region: Some(region_index),
                start: 0,
                end: 0,
                text: String::new(),
            });
            continue;
        }
        let mut start = 0;
        let mut used = 0;
        for (byte, character) in region.text.char_indices() {
            if character == '\n' {
                out.push(VisualLine {
                    region: Some(region_index),
                    start,
                    end: byte,
                    text: region.text[start..byte].into(),
                });
                start = byte + 1;
                used = 0;
                continue;
            }
            let char_width = character.width().unwrap_or(0);
            if used > 0 && used + char_width > width {
                out.push(VisualLine {
                    region: Some(region_index),
                    start,
                    end: byte,
                    text: region.text[start..byte].into(),
                });
                start = byte;
                used = 0;
            }
            used += char_width;
        }
        out.push(VisualLine {
            region: Some(region_index),
            start,
            end: region.text.len(),
            text: region.text[start..].into(),
        });
    }
    out
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

    #[test]
    fn ctrl_shift_chord_does_not_start_leap_or_cancel_selection() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        assert!(app.editor.insert("selected"));
        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        app.editor.set_cursor(
            Cursor {
                region: 0,
                byte: "selected".len(),
            },
            true,
        );
        let selection = app.editor.selection();
        let mut clipboard = Clipboard::default();
        let mut dispatcher = Dispatcher::default();

        handle_key(
            &mut app,
            &mut clipboard,
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
            &mut clipboard,
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
            &mut clipboard,
            &mut dispatcher,
            KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ),
            true,
        )
        .unwrap();

        assert!(matches!(app.mode, AppMode::Editing));
        assert_eq!(app.editor.selection(), selection);
        assert!(dispatcher.pending_control.is_none());
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
