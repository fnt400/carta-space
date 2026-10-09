mod input;
mod layout;
mod markdown;
mod profile;
mod session;
mod ui;
mod viewport;

use base64::Engine as _;
use carta_app::{App, AppMode, Command, Session, View};
use carta_core::{stage_sync, Archive, StagedSync, SyncApply, SyncOutcome};
use iced::event::{self, Status};
use iced::theme::Mode as ThemeMode;
use iced::widget::operation::{self, AbsoluteOffset};
use iced::{system, window, Event, Font, Point, Settings, Size, Subscription, Task, Theme};
use input::{ClipboardRequest, GuiInputState};
use std::cell::RefCell;
use std::env;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn main() -> iced::Result {
    iced::application(Gui::boot, update, ui::view)
        .title("Carta Space")
        .theme(theme)
        .settings(Settings {
            default_font: Font::with_name("Iosevka"),
            ..Settings::default()
        })
        .subscription(subscription)
        .window(window::Settings {
            exit_on_close_request: false,
            ..window::Settings::default()
        })
        .run()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum QuitState {
    Idle,
    WaitingForCurrentSync,
    Publishing,
    Failed(String),
}

pub(crate) struct Gui {
    pub(crate) app: Option<App>,
    pub(crate) input: GuiInputState,
    pub(crate) error: Option<String>,
    pub(crate) setup_path: Option<PathBuf>,
    pub(crate) clone_url: String,
    pub(crate) theme_mode: ThemeMode,
    pub(crate) window_size: Size,
    pub(crate) font_size: f32,
    pub(crate) markdown: RefCell<markdown::Cache>,
    pub(crate) layout: RefCell<layout::Layout>,
    pub(crate) scroll_y: f32,
    session_root: Option<PathBuf>,
    last_saved_session: Option<Session>,
    pointer: Option<Point>,
    pointer_down: bool,
    sync_active: bool,
    clone_active: bool,
    sync_ready: Option<StagedSync>,
    next_sync_at: Instant,
    last_user_input: Instant,
    sync_failures: u32,
    sync_started_generation: u64,
    pub(crate) quit_state: QuitState,
    quit_retries: u8,
    pub(crate) sync_error: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    Raw(Event),
    Tick(Instant),
    SystemTheme(ThemeMode),
    FontLoaded(bool),
    WindowOpened(window::Id),
    WindowSize(Size),
    PasteText(Option<String>),
    PointerMoved(Point),
    PointerPressed,
    PointerReleased,
    Scrolled(f32),
    SyncFinished(Result<StagedSync, String>),
    QuitRequested,
    QuitRetry,
    QuitCancel,
    QuitWithoutSync,
    CloneUrlChanged(String),
    CreateDefaultArchive,
    CloneDefaultArchive,
    CloneFinished(Result<(), String>),
}

impl Gui {
    fn boot() -> (Self, Task<Message>) {
        // Keep the desktop appearance independent of installed system fonts.
        let font_bytes = base64::engine::general_purpose::STANDARD
            .decode(include_str!("../assets/iosevka-regular.ttf.b64").trim())
            .expect("bundled Iosevka font must be valid base64");
        let mut gui = Self::load();
        let sync = service_tick(&mut gui, Instant::now());
        (
            gui,
            Task::batch([
                system::theme().map(Message::SystemTheme),
                iced::font::load(font_bytes).map(|result| Message::FontLoaded(result.is_ok())),
                sync,
            ]),
        )
    }

    fn load() -> Self {
        let missing = || Self {
            app: None,
            input: GuiInputState::default(),
            error: Some("Unable to resolve the default archive path".into()),
            setup_path: None,
            clone_url: String::new(),
            theme_mode: ThemeMode::Dark,
            window_size: Size::new(1024.0, 768.0),
            font_size: viewport::FONT_SIZE,
            markdown: RefCell::new(markdown::Cache::default()),
            layout: RefCell::new(layout::Layout::default()),
            scroll_y: 0.0,
            session_root: None,
            last_saved_session: None,
            pointer: None,
            pointer_down: false,
            sync_active: false,
            clone_active: false,
            sync_ready: None,
            next_sync_at: Instant::now(),
            last_user_input: Instant::now(),
            sync_failures: 0,
            sync_started_generation: 0,
            quit_state: QuitState::Idle,
            quit_retries: 0,
            sync_error: None,
        };
        let explicit_archive = env::args_os().nth(1);
        let Some(path) = explicit_archive
            .clone()
            .map(PathBuf::from)
            .or_else(|| session::default_archive_path().ok())
        else {
            return missing();
        };

        let root = session::data_root().ok();
        let setup_path = (explicit_archive.is_none() && !path.exists()).then_some(path.clone());
        let mut sync_error = None;
        let result = if setup_path.is_some() {
            Err(String::new())
        } else {
            Archive::open(&path)
                .map_err(|error| error.to_string())
                .and_then(|archive| {
                    if let Err(error) = archive.enable_origin_sync_remote() {
                        sync_error = Some(format!("Sync setup: {error}"));
                    }
                    let previous = root.as_deref().and_then(|root| {
                        session::load(root, archive.metadata().archive_id())
                            .ok()
                            .flatten()
                    });
                    App::open(archive, previous.as_ref(), Instant::now())
                        .map(|mut app| {
                            app.enable_background_sync();
                            (app, previous)
                        })
                        .map_err(|error| error.to_string())
                })
        };

        let (app, error, previous) = match result {
            Ok((app, previous)) => (Some(app), None, previous),
            Err(_) if setup_path.is_some() => (None, None, None),
            Err(error) => (None, Some(error), None),
        };
        Self {
            app,
            input: GuiInputState::default(),
            error,
            setup_path,
            clone_url: String::new(),
            theme_mode: ThemeMode::Dark,
            window_size: Size::new(1024.0, 768.0),
            font_size: viewport::FONT_SIZE,
            markdown: RefCell::new(markdown::Cache::default()),
            layout: RefCell::new(layout::Layout::default()),
            scroll_y: 0.0,
            session_root: root,
            last_saved_session: previous,
            pointer: None,
            pointer_down: false,
            sync_active: false,
            clone_active: false,
            sync_ready: None,
            next_sync_at: Instant::now()
                + if sync_error.is_some() {
                    Duration::from_secs(30)
                } else {
                    Duration::ZERO
                },
            last_user_input: Instant::now(),
            sync_failures: u32::from(sync_error.is_some()),
            sync_started_generation: 0,
            quit_state: QuitState::Idle,
            quit_retries: 0,
            sync_error,
        }
    }

    fn install_archive(&mut self, archive: Archive) {
        if let Err(error) = archive.enable_origin_sync_remote() {
            self.sync_error = Some(format!("Sync setup: {error}"));
            self.sync_failures = 1;
        }
        let previous = self.session_root.as_deref().and_then(|root| {
            session::load(root, archive.metadata().archive_id())
                .ok()
                .flatten()
        });
        match App::open(archive, previous.as_ref(), Instant::now()) {
            Ok(mut app) => {
                app.enable_background_sync();
                self.app = Some(app);
                self.last_saved_session = previous;
                self.error = None;
                self.setup_path = None;
                self.next_sync_at = Instant::now()
                    + if self.sync_failures > 0 {
                        Duration::from_secs(30)
                    } else {
                        Duration::ZERO
                    };
            }
            Err(error) => self.error = Some(format!("Archive open failed: {error}")),
        }
    }

    fn create_default_archive(&mut self) {
        let Some(path) = self.setup_path.as_ref() else {
            return;
        };
        if path.exists() {
            self.error = Some("The destination already exists; no files were changed".into());
            return;
        }
        // $XDG_DATA_HOME/carta may not exist on a first installation.
        let result = path
            .parent()
            .map(std::fs::create_dir_all)
            .unwrap_or(Ok(()))
            .map_err(|error| error.to_string())
            .and_then(|()| Archive::create(path).map_err(|error| error.to_string()));
        match result {
            Ok(archive) => self.install_archive(archive),
            Err(error) => self.error = Some(format!("Archive setup failed: {error}")),
        }
    }

    fn save_session(&mut self) -> std::io::Result<()> {
        let (Some(root), Some(app)) = (&self.session_root, &self.app) else {
            return Ok(());
        };
        if app.kill_switch_triggered() {
            return Ok(());
        }
        let current = app.session();
        if self.last_saved_session.as_ref() != Some(&current) {
            session::save(root, app.archive.metadata().archive_id(), &current)?;
            self.last_saved_session = Some(current);
        }
        Ok(())
    }
}

impl Drop for Gui {
    fn drop(&mut self) {
        if let Some(app) = &mut self.app {
            if !app.kill_switch_triggered() {
                if let Err(error) = app.autosave() {
                    eprintln!("carta-gui: final autosave failed: {error}");
                }
            }
        }
        if let Err(error) = self.save_session() {
            eprintln!("carta-gui: final session save failed: {error}");
        }
    }
}

fn subscription(state: &Gui) -> Subscription<Message> {
    let keyboard = event::listen_raw(|event, _status: Status, _window: window::Id| {
        matches!(event, Event::Keyboard(_)).then_some(Message::Raw(event))
    });

    // Avoid periodic reconstruction of the entire rich-text view while idle.
    // Dirty buffers are saved promptly; transient statuses still expire; an
    // otherwise idle window needs only infrequent local maintenance.
    let interval = if state.app.as_ref().is_some_and(|app| app.editor.is_dirty()) {
        Duration::from_secs(1)
    } else if state.app.as_ref().is_some_and(|app| !app.status.is_empty()) {
        Duration::from_secs(2)
    } else {
        Duration::from_secs(15)
    };
    // A mouse drag can end outside the text sheet; release the selection
    // state even if the sheet's mouse area does not receive the event.
    let mouse_release = event::listen_raw(|event, _status, _window| {
        matches!(
            event,
            Event::Mouse(iced::mouse::Event::ButtonReleased(
                iced::mouse::Button::Left
            ))
        )
        .then_some(Message::PointerReleased)
    });
    Subscription::batch([
        keyboard,
        mouse_release,
        iced::time::every(interval).map(Message::Tick),
        window::open_events().map(Message::WindowOpened),
        window::close_requests().map(|_| Message::QuitRequested),
        window::resize_events().map(|(_id, size)| Message::WindowSize(size)),
        system::theme_changes().map(Message::SystemTheme),
    ])
}

fn scroll_to_caret(state: &mut Gui) -> Task<Message> {
    let Some(app) = &state.app else {
        return Task::none();
    };
    let started = profile::enabled().then(Instant::now);
    let mut layout = state.layout.borrow_mut();
    layout.sync(
        app,
        viewport::columns(state.window_size.width, state.font_size),
    );
    let row = layout.caret_row(app);
    let offset = viewport::VERTICAL_PADDING + row as f32 * viewport::line_height(state.font_size);
    drop(layout);
    state.scroll_y = offset;
    if let Some(started) = started {
        profile::record("scroll", started);
    }
    operation::scroll_to(
        ui::EDITOR_SCROLL_ID,
        AbsoluteOffset {
            x: None,
            y: Some(offset),
        },
    )
}

/// Map pointer coordinates in the *rendered window* to the Archive's
/// canonical insertion point. No scanning over preceding Documents.
fn indexed_hit_test(
    app: &App,
    cache: &RefCell<layout::Layout>,
    scroll_y: f32,
    point: Point,
    window: Size,
    font_size: f32,
) -> Option<carta_app::Cursor> {
    let line_height = viewport::line_height(font_size);
    let height = viewport::writing_area_height(window.height);
    let top = height * (2.0 / 3.0) + viewport::VERTICAL_PADDING;
    let mut layout = cache.borrow_mut();
    layout.sync(app, viewport::columns(window.width, font_size));
    let (first, _) = layout.window(scroll_y, height, line_height, top);
    let row = first.saturating_add((point.y.max(0.0) / line_height) as usize);
    let left = (window.width - window.width.min(viewport::PAGE_WIDTH)) / 2.0
        + viewport::HORIZONTAL_PADDING;
    let col = ((point.x - left).max(0.0) / viewport::mono_advance(font_size)).round() as usize;
    layout.hit_test(app, row, col)
}

/// Background transfers are NOT gated by keyboard idleness. Each committed
/// change queues a push of the immutable Git HEAD even if the user continues
/// typing or the current Document is provisional. Only the *integration* of
/// remote changes may wait for a safe moment.
fn service_tick(state: &mut Gui, now: Instant) -> Task<Message> {
    if state.quit_state != QuitState::Idle {
        return Task::none();
    }
    if state.app.as_ref().is_some_and(|app| app.quit) {
        return request_quit(state);
    }
    const QUIET: Duration = Duration::from_secs(2);
    const INTERVAL: Duration = Duration::from_secs(180);
    let idle = now.saturating_duration_since(state.last_user_input) >= QUIET && !state.pointer_down;

    if state.sync_ready.is_some() {
        let current_generation = state
            .app
            .as_ref()
            .map(|app| app.scheduler.sync_generation())
            .unwrap_or_default();

        // A newer checkpoint supersedes an incoming snapshot while the
        // editor is busy. Preserve the new pending push and redo the remote
        // merge using the new committed base.
        if current_generation != state.sync_started_generation {
            state.sync_ready = None;
            state.sync_failures = state.sync_failures.max(1);
            state.next_sync_at = now + Duration::from_secs(5);
        } else {
            let can_integrate = state.app.as_ref().is_some_and(|app| {
                !app.editor.is_dirty()
                    && matches!(app.mode, AppMode::Editing)
                    && app.conflicts.is_empty()
            });
            if idle && can_integrate {
                let staged = state.sync_ready.take().expect("checked above");
                let result = if let Some(app) = state.app.as_mut() {
                    staged
                        .configuration_matches(&app.archive)
                        .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })
                        .and_then(|matches| {
                            if matches {
                                app.apply_background_sync(&staged)
                            } else {
                                Ok(SyncApply::Stale)
                            }
                        })
                } else {
                    return Task::none();
                };
                match result {
                    Ok(SyncApply::Updated) => {
                        state.sync_error = None;
                        state.sync_failures = 0;
                        *state.layout.borrow_mut() = layout::Layout::default();
                        *state.markdown.borrow_mut() = markdown::Cache::default();
                        state.next_sync_at = now + INTERVAL;
                        return scroll_to_caret(state);
                    }
                    Ok(SyncApply::Unchanged) => {
                        state.sync_error = None;
                        state.sync_failures = 0;
                        state.next_sync_at = now + INTERVAL;
                    }
                    Ok(SyncApply::Conflict) => {
                        state.sync_error =
                            Some("Conflitto Git: modifiche simultanee da risolvere".into());
                        state.sync_failures = state.sync_failures.max(1);
                        state.next_sync_at = now + INTERVAL;
                    }
                    Ok(SyncApply::Stale) => {
                        // The editor/Archive changed after staging: retry
                        // without replacing any live data, but avoid an
                        // immediate worker loop while those changes settle.
                        state.sync_failures = state.sync_failures.max(1);
                        state.sync_error =
                            Some("Sync: archive or configuration changed; retry queued".into());
                        state.next_sync_at = now + Duration::from_secs(5);
                    }
                    Err(error) => {
                        state.sync_error = Some(format!("Sync: {error}"));
                        sync_backoff(state, now);
                    }
                }
            }
        }
    }

    if state.sync_active || state.sync_ready.is_some() {
        return Task::none();
    }
    let Some(app) = &mut state.app else {
        return Task::none();
    };
    let pending = app.scheduler.is_sync_pending();
    if now < state.next_sync_at && (state.sync_failures > 0 || !pending) {
        return Task::none();
    }

    // Adoption is local-only and optional. Failure never closes the editor.
    if let Err(error) = app.archive.enable_origin_sync_remote() {
        state.sync_error = Some(format!("Sync setup: {error}"));
        sync_backoff(state, now);
        return Task::none();
    }

    // Before starting an outbound transfer we may checkpoint a fully saved,
    // non-provisional buffer. Otherwise publish only already committed data.
    match app.prepare_background_sync() {
        Ok(true) => {}
        Ok(false) => {
            state.next_sync_at = now + INTERVAL;
            // Keep the checkpoint queued without probing missing configuration
            // again on every keystroke.
            state.sync_failures = state.sync_failures.max(1);
            return Task::none();
        }
        Err(error) => {
            state.sync_error = Some(format!("Sync: {error}"));
            sync_backoff(state, now);
            return Task::none();
        }
    }

    state.sync_started_generation = app.scheduler.sync_generation();
    let root = app.archive.root().to_path_buf();
    state.sync_active = true;
    state.next_sync_at = now + INTERVAL;
    Task::perform(
        detached_job(move || stage_sync(root).map_err(|error| error.to_string())),
        Message::SyncFinished,
    )
}

/// Normal Quit closes only after publishing the final checkpoint or after
/// explicit acknowledgement that publication has not succeeded.
fn request_quit(state: &mut Gui) -> Task<Message> {
    // The explicit emergency kill switch is not a normal Quit and must
    // remain immediately available when the user requests it.
    if state
        .app
        .as_ref()
        .is_some_and(|app| app.kill_switch_triggered())
    {
        return finish_quit(state);
    }
    if state.quit_state != QuitState::Idle {
        return Task::none();
    }
    if let Some(app) = state.app.as_mut() {
        if !app.quit {
            if matches!(app.mode, AppMode::Leap { .. }) {
                app.cancel_mode();
            }
            if let Err(error) = app.execute(Command::Quit) {
                app.status = format!("Quit: {error}");
                return Task::none();
            }
        }
    } else {
        return finish_quit(state);
    }
    if let Err(error) = state.save_session() {
        return quit_failed(state, format!("Salvataggio sessione fallito: {error}"));
    }
    // An earlier staged snapshot does not include the final Quit checkpoint.
    state.sync_ready = None;
    state.quit_retries = 0;
    start_quit_publication(state)
}

fn quit_failed(state: &mut Gui, error: String) -> Task<Message> {
    state.sync_error = Some(error.clone());
    state.quit_state = QuitState::Failed(error);
    Task::none()
}

fn start_quit_publication(state: &mut Gui) -> Task<Message> {
    if state.sync_active {
        state.quit_state = QuitState::WaitingForCurrentSync;
        return Task::none();
    }
    let Some(app) = state.app.as_mut() else {
        return finish_quit(state);
    };
    // An earlier setup error cannot be interpreted as successful publication.
    if let Err(error) = app.archive.enable_origin_sync_remote() {
        return quit_failed(state, format!("Configurazione Git fallita: {error}"));
    }
    match app.archive.sync_remote() {
        Ok(None) => return finish_quit(state), // Explicit local-only mode.
        Err(error) => return quit_failed(state, format!("Controllo remoto fallito: {error}")),
        Ok(Some(_)) => {}
    }
    // App::finish_quit already saved and checkpointed the last text.
    // prepare_background_sync() deliberately skips jobs after app.quit.
    let root = app.archive.root().to_path_buf();
    state.sync_started_generation = app.scheduler.sync_generation();
    state.sync_active = true;
    state.quit_state = QuitState::Publishing;
    Task::perform(
        detached_job(move || stage_sync(root).map_err(|error| error.to_string())),
        Message::SyncFinished,
    )
}

fn finish_quit(state: &mut Gui) -> Task<Message> {
    state.input.cancel_leap();
    state.pointer_down = false;
    if let Err(error) = state.save_session() {
        eprintln!("carta-gui: final session save failed: {error}");
    }
    iced::exit()
}

fn sync_backoff(state: &mut Gui, now: Instant) {
    state.sync_failures = state.sync_failures.saturating_add(1);
    state.next_sync_at = now + Duration::from_secs(30 * (1_u64 << state.sync_failures.min(4)));
}

// Dropping the async receiver does not join the network thread. In particular,
// Tokio shutdown cannot wait indefinitely for a Git/SSH or clone operation.
async fn detached_job<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (sender, receiver) = iced::futures::channel::oneshot::channel();
    std::thread::Builder::new()
        .name("carta-network".into())
        .spawn(move || {
            let _ = sender.send(job());
        })
        .map_err(|error| format!("Network worker failed: {error}"))?;
    receiver
        .await
        .map_err(|_| "Network worker stopped without a result".to_owned())?
}

fn normalize_clipboard(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

fn update(state: &mut Gui, message: Message) -> Task<Message> {
    match message {
        Message::QuitRequested => return request_quit(state),
        Message::QuitRetry => {
            if matches!(&state.quit_state, QuitState::Failed(_)) {
                state.quit_retries = 0;
                return start_quit_publication(state);
            }
            return Task::none();
        }
        Message::QuitCancel => {
            if state.quit_state != QuitState::Idle {
                state.quit_state = QuitState::Idle;
                if let Some(app) = state.app.as_mut() {
                    app.quit = false;
                    app.status = "Uscita annullata · commit locali conservati".into();
                }
                return service_tick(state, Instant::now());
            }
            return Task::none();
        }
        Message::QuitWithoutSync => {
            if state.quit_state != QuitState::Idle {
                // Explicit acknowledgement is required for offline exit.
                return finish_quit(state);
            }
            return Task::none();
        }
        Message::CloneUrlChanged(url) => {
            state.clone_url = url;
            state.error = None;
            return Task::none();
        }
        Message::CreateDefaultArchive => {
            if state.clone_active {
                return Task::none();
            }
            state.create_default_archive();
            return service_tick(state, Instant::now());
        }
        Message::CloneDefaultArchive => {
            if state.clone_active {
                return Task::none();
            }
            let Some(path) = state.setup_path.clone() else {
                return Task::none();
            };
            let remote = state.clone_url.trim().to_owned();
            if remote.is_empty() {
                state.error = Some("Specify the Git remote URL to clone".into());
                return Task::none();
            }
            if path.exists() {
                state.error = Some("The destination already exists; no files were changed".into());
                return Task::none();
            }
            state.error = Some("Cloning Git archive…".into());
            state.clone_active = true;
            return Task::perform(
                detached_job(move || {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                    }
                    Archive::clone_sync_remote(&remote, &path)
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                }),
                Message::CloneFinished,
            );
        }
        Message::CloneFinished(result) => {
            state.clone_active = false;
            match result {
                Ok(()) => {
                    if let Some(path) = state.setup_path.as_ref() {
                        match Archive::open(path) {
                            Ok(archive) => state.install_archive(archive),
                            Err(error) => {
                                state.error = Some(format!("Archive open failed: {error}"));
                            }
                        }
                    }
                }
                Err(error) => state.error = Some(format!("Git clone failed: {error}")),
            }
            return service_tick(state, Instant::now());
        }
        Message::SyncFinished(result) => {
            state.sync_active = false;
            let now = Instant::now();
            if state.quit_state == QuitState::WaitingForCurrentSync {
                // The old worker may have published only the pre-Quit HEAD.
                // Always run a fresh worker after its completion.
                return start_quit_publication(state);
            }
            if state.quit_state == QuitState::Publishing {
                let stage = match result {
                    Ok(stage) => stage,
                    Err(error) => {
                        return quit_failed(state, format!("Push finale fallito: {error}"))
                    }
                };
                let Some(app) = state.app.as_mut() else {
                    return quit_failed(state, "Archivio chiuso durante il push".into());
                };
                let outcome = stage.outcome();
                // Validates the exact local HEAD, archive cleanliness, and
                // pinned remote configuration before acknowledging this result.
                let applied = app.apply_background_sync(&stage);
                return match applied {
                    Ok(SyncApply::Unchanged)
                        if matches!(outcome, SyncOutcome::Synced | SyncOutcome::Published) =>
                    {
                        state.sync_error = None;
                        finish_quit(state)
                    }
                    Ok(SyncApply::Updated) | Ok(SyncApply::Stale) | Ok(SyncApply::Unchanged) => {
                        // Recheck the current HEAD after a merge or stale stage,
                        // not the obsolete one published by an older worker.
                        state.quit_retries += 1;
                        if state.quit_retries >= 3 {
                            quit_failed(state, "Push finale non verificato dopo 3 tentativi".into())
                        } else {
                            start_quit_publication(state)
                        }
                    }
                    Ok(SyncApply::Conflict) => quit_failed(
                        state,
                        "Conflitto Git durante il push finale: risoluzione necessaria".into(),
                    ),
                    Err(error) => quit_failed(state, format!("Verifica push finale: {error}")),
                };
            }
            let result = result.and_then(|stage| {
                let Some(app) = state.app.as_ref() else {
                    return Err("Sync archive is no longer open".into());
                };
                if stage
                    .configuration_matches(&app.archive)
                    .map_err(|error| error.to_string())?
                {
                    Ok(stage)
                } else {
                    Err("Sync configuration changed; retry queued".into())
                }
            });
            match result {
                Ok(stage) => match stage.outcome() {
                    SyncOutcome::Synced | SyncOutcome::Published => {
                        // Publication changes no live files. Acknowledge only
                        // this generation without waiting for editing to stop.
                        if let Some(app) = state.app.as_mut() {
                            app.scheduler
                                .sync_finished(state.sync_started_generation, now);
                        }
                        state.sync_error = None;
                        state.sync_failures = 0;
                        state.next_sync_at = now + Duration::from_secs(180);
                    }
                    SyncOutcome::Conflict => {
                        state.sync_error =
                            Some("Conflitto Git: modifiche concorrenti sul server".into());
                        state.sync_failures = state.sync_failures.max(1);
                        state.next_sync_at = now + Duration::from_secs(180);
                    }
                    _ => {
                        // The worker downloaded/merged data. Wait until the
                        // editor is safe before updating the live Archive.
                        state.sync_ready = Some(stage);
                    }
                },
                Err(error) => {
                    state.sync_error = Some(format!("Push/pull Git fallito: {error}"));
                    sync_backoff(state, now);
                    // Crucially, do not acknowledge sync_pending after failure.
                    // It must remain queued through all retry attempts.
                }
            }
            return service_tick(state, now);
        }
        Message::Scrolled(offset) => {
            // Native wheel and scrollbar position remains the source of truth.
            // Only the bounded visible-row window is recomputed.
            state.scroll_y = offset.max(0.0);
            return Task::none();
        }
        Message::PointerMoved(point) => {
            state.pointer = Some(point);
            if state.pointer_down {
                state.last_user_input = Instant::now();
            }
            if state.pointer_down {
                if let Some(app) = &mut state.app {
                    if matches!(app.mode, AppMode::Editing)
                        && !app.collapsed
                        && matches!(
                            app.view,
                            View::CreationDate(_) | View::ModificationDate | View::Work(_)
                        )
                    {
                        if let Some(cursor) = indexed_hit_test(
                            app,
                            &state.layout,
                            state.scroll_y,
                            point,
                            state.window_size,
                            state.font_size,
                        ) {
                            if cursor != app.editor.cursor() {
                                app.editor.set_cursor(cursor, true);
                            }
                        }
                    }
                }
            }
            return Task::none();
        }
        Message::PointerPressed => {
            state.last_user_input = Instant::now();
            let Some(point) = state.pointer else {
                return Task::none();
            };
            if let Some(app) = &mut state.app {
                if matches!(app.mode, AppMode::Editing)
                    && !app.collapsed
                    && matches!(
                        app.view,
                        View::CreationDate(_) | View::ModificationDate | View::Work(_)
                    )
                {
                    if let Some(cursor) = indexed_hit_test(
                        app,
                        &state.layout,
                        state.scroll_y,
                        point,
                        state.window_size,
                        state.font_size,
                    ) {
                        app.cat_navigation();
                        app.editor.set_cursor(cursor, false);
                        state.pointer_down = true;
                    }
                }
            }
            return Task::none();
        }
        Message::PointerReleased => {
            state.pointer_down = false;
            state.last_user_input = Instant::now();
            return Task::none();
        }
        Message::Tick(now) => {
            if state.quit_state != QuitState::Idle {
                return Task::none();
            }
            if let Some(app) = &mut state.app {
                let started = profile::enabled().then(Instant::now);
                if let Err(error) = app.tick_without_remote_sync(now) {
                    app.status = format!("Error: {error}");
                }
                if let Some(started) = started {
                    profile::record("maintenance", started);
                }
            }
            if let Err(error) = state.save_session() {
                if let Some(app) = &mut state.app {
                    app.status = format!("Session save: {error}");
                }
            }
            return service_tick(state, now);
        }
        Message::SystemTheme(mode) => {
            state.theme_mode = mode;
            return Task::none();
        }
        Message::FontLoaded(false) => {
            if let Some(app) = &mut state.app {
                app.status = "Bundled Iosevka font could not be loaded".into();
            }
            return Task::none();
        }
        Message::FontLoaded(true) => return scroll_to_caret(state),
        Message::WindowOpened(id) => {
            return window::size(id).map(Message::WindowSize);
        }
        Message::WindowSize(size) => {
            state.window_size = size;
            return scroll_to_caret(state);
        }
        Message::PasteText(value) => {
            if let Some(app) = &mut state.app {
                if matches!(app.mode, carta_app::AppMode::Editing) {
                    if let Some(value) = value {
                        let value = normalize_clipboard(&value);
                        if !value.is_empty() {
                            app.dispatch_action(
                                carta_app::Action::InsertText(value),
                                Instant::now(),
                            );
                            return scroll_to_caret(state);
                        }
                    }
                    app.status = "No text available in system clipboard".into();
                }
            }
            return Task::none();
        }
        _ => {}
    }

    if state.quit_state != QuitState::Idle {
        // Freeze editor input while publishing or showing the failure warning.
        return Task::none();
    }
    let columns = viewport::columns(state.window_size.width, state.font_size);
    let Some(app) = &mut state.app else {
        return Task::none();
    };
    let keyboard_event = matches!(message, Message::Raw(Event::Keyboard(_)));
    if keyboard_event {
        state.last_user_input = Instant::now();
    }
    let before_cursor = app.editor.cursor();
    let before_region_count = app.editor.regions().len();
    let before_view = app.view.clone();
    let before_collapsed = app.collapsed;
    let before_mode = std::mem::discriminant(&app.mode);
    let mut clipboard = Task::none();
    let mut zoomed = false;
    let result = match message {
        Message::Raw(Event::Keyboard(event)) => {
            let started = profile::enabled().then(Instant::now);
            let result = state.input.handle(app, event, columns);
            let zoom = state.input.take_zoom_request();
            if zoom != 0 {
                let next = (state.font_size + f32::from(zoom) * viewport::FONT_STEP)
                    .clamp(viewport::MIN_FONT_SIZE, viewport::MAX_FONT_SIZE);
                zoomed = next != state.font_size;
                state.font_size = next;
                app.status = format!("Writing font: {next:.0} px");
            }
            if let Some(started) = started {
                profile::record("input", started);
            }
            clipboard = match state.input.take_clipboard_request() {
                Some(ClipboardRequest::Read) => iced::clipboard::read().map(Message::PasteText),
                Some(ClipboardRequest::Write(text)) => iced::clipboard::write(text),
                None => Task::none(),
            };
            result
        }
        _ => Ok(()),
    };
    if let Err(error) = result {
        app.status = format!("Error: {error}");
    }
    let follow_caret = zoomed
        || (keyboard_event
            && (app.editor.cursor() != before_cursor
                || app.editor.regions().len() != before_region_count
                || app.view != before_view
                || app.collapsed != before_collapsed
                || std::mem::discriminant(&app.mode) != before_mode));
    if app.quit {
        request_quit(state)
    } else {
        let auto_sync = service_tick(state, Instant::now());
        if follow_caret {
            Task::batch([clipboard, scroll_to_caret(state), auto_sync])
        } else {
            Task::batch([clipboard, auto_sync])
        }
    }
}

fn theme(state: &Gui) -> Theme {
    match state.theme_mode {
        ThemeMode::Light => Theme::Light,
        ThemeMode::Dark | ThemeMode::None => Theme::Dark,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use carta_core::CheckpointKind;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TemporaryArchive(PathBuf);

    impl TemporaryArchive {
        fn new() -> Self {
            static NEXT_ID: AtomicU64 = AtomicU64::new(0);
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
            let path = env::temp_dir().join(format!(
                "carta-gui-regression-{}-{}-{}",
                std::process::id(),
                stamp.as_nanos(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TemporaryArchive {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn git(path: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .current_dir(path)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn fixture() -> (Gui, TemporaryArchive) {
        let temporary = TemporaryArchive::new();
        let remote = temporary.0.join("remote.git");
        git(
            &temporary.0,
            &["init", "--bare", "--quiet", remote.to_str().unwrap()],
        );
        let path = temporary.0.join("archive");
        let mut archive = Archive::create(&path).unwrap();
        git(&path, &["branch", "-M", "carta"]);
        git(
            &path,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        git(&path, &["config", "branch.carta.remote", "origin"]);
        git(&path, &["config", "branch.carta.merge", "refs/heads/carta"]);
        git(
            &path,
            &["push", "--quiet", "origin", "HEAD:refs/heads/carta"],
        );
        archive
            .create_document("synthetic existing checkpoint")
            .unwrap();
        archive
            .checkpoint(CheckpointKind::Structural, Some("synthetic checkpoint"))
            .unwrap();
        assert!(!archive.is_dirty().unwrap());
        assert!(archive.sync_remote().unwrap().is_none());
        let now = Instant::now();
        let mut gui = Gui {
            app: None,
            input: GuiInputState::default(),
            error: None,
            setup_path: None,
            clone_url: String::new(),
            theme_mode: ThemeMode::Dark,
            window_size: Size::new(1024.0, 768.0),
            font_size: viewport::FONT_SIZE,
            markdown: RefCell::new(markdown::Cache::default()),
            layout: RefCell::new(layout::Layout::default()),
            scroll_y: 0.0,
            session_root: Some(temporary.0.join("sessions")),
            last_saved_session: None,
            pointer: None,
            pointer_down: false,
            sync_active: false,
            clone_active: false,
            sync_ready: None,
            next_sync_at: now,
            last_user_input: now,
            sync_failures: 0,
            sync_started_generation: 0,
            quit_state: QuitState::Idle,
            quit_retries: 0,
            sync_error: None,
        };
        gui.install_archive(archive);
        gui.last_user_input = now - Duration::from_secs(3);
        assert!(gui.error.is_none(), "{:?}", gui.error);
        assert_eq!(
            gui.app
                .as_ref()
                .unwrap()
                .archive
                .sync_remote()
                .unwrap()
                .as_deref(),
            remote.to_str()
        );
        (gui, temporary)
    }

    fn assert_published(gui: &Gui, temporary: &TemporaryArchive) {
        let app = gui.app.as_ref().unwrap();
        assert_eq!(
            git(
                &temporary.0.join("remote.git"),
                &["rev-parse", "refs/heads/carta"]
            ),
            git(app.archive.root(), &["rev-parse", "HEAD"])
        );
        assert!(!app.archive.is_dirty().unwrap());
        assert!(!app.editor.is_dirty());
    }

    fn incoming_stage(gui: &Gui, temporary: &TemporaryArchive) -> StagedSync {
        let path = gui.app.as_ref().unwrap().archive.root().to_path_buf();
        stage_sync(path.clone()).unwrap();
        let mut peer = Archive::clone_sync_remote(
            temporary.0.join("remote.git").to_str().unwrap(),
            temporary.0.join("peer"),
        )
        .unwrap();
        peer.create_document("synthetic incoming document").unwrap();
        peer.checkpoint(CheckpointKind::Structural, None).unwrap();
        peer.sync().unwrap();
        let staged = stage_sync(path).unwrap();
        assert_eq!(staged.outcome(), SyncOutcome::UpdatedFromRemote);
        staged
    }

    #[test]
    fn automatic_sync_publishes_clean_existing_unpushed_checkpoint() {
        let (mut gui, temporary) = fixture();
        let path = gui.app.as_ref().unwrap().archive.root().to_path_buf();
        let head = git(&path, &["rev-parse", "HEAD"]);
        assert_ne!(
            git(
                &temporary.0.join("remote.git"),
                &["rev-parse", "refs/heads/carta"]
            ),
            head
        );
        assert!(!gui.app.as_ref().unwrap().editor.is_dirty());
        assert!(service_tick(&mut gui, Instant::now()).units() > 0);
        assert!(gui.sync_active);
        assert_eq!(git(&path, &["rev-parse", "HEAD"]), head);
        assert_eq!(service_tick(&mut gui, Instant::now()).units(), 0);
        let staged = stage_sync(path).unwrap();
        assert_eq!(staged.outcome(), SyncOutcome::Published);
        assert_published(&gui, &temporary);
        assert_eq!(
            update(&mut gui, Message::SyncFinished(Ok(staged))).units(),
            0
        );
        assert!(!gui.sync_active);
        assert!(gui.sync_error.is_none());
        assert!(!gui.app.as_ref().unwrap().scheduler.is_sync_pending());
    }

    #[test]
    fn window_close_publishes_the_final_checkpoint_and_updates_git_status() {
        let (mut gui, temporary) = fixture();
        let app = gui.app.as_mut().unwrap();
        let path = app.archive.root().to_path_buf();
        let before = git(&path, &["rev-parse", "HEAD"]);
        assert!(app.editor.insert("synthetic final text"));
        assert!(update(&mut gui, Message::QuitRequested).units() > 0);
        assert!(gui.app.as_ref().unwrap().quit);
        assert_eq!(gui.quit_state, QuitState::Publishing);
        assert!(gui.sync_active);
        assert_ne!(git(&path, &["rev-parse", "HEAD"]), before);
        assert!(!gui.app.as_ref().unwrap().archive.is_dirty().unwrap());
        let final_stage = stage_sync(path.clone()).unwrap();
        assert_eq!(final_stage.outcome(), SyncOutcome::Published);
        assert!(update(&mut gui, Message::SyncFinished(Ok(final_stage))).units() > 0);
        assert_published(&gui, &temporary);
        assert_eq!(
            git(&path, &["rev-parse", "refs/remotes/carta-sync/carta"]),
            git(&path, &["rev-parse", "HEAD"]),
            "tracking ref must reflect the confirmed push"
        );
        assert_eq!(
            git(&path, &["rev-list", "--count", "carta-sync/carta..HEAD"]),
            "0",
            "git status must not report a stale unpushed commit"
        );
    }

    #[test]
    fn unavailable_remote_blocks_quit_until_user_acknowledges_failure() {
        let (mut gui, temporary) = fixture();
        let app = gui.app.as_mut().unwrap();
        let path = app.archive.root().to_path_buf();
        assert!(app.editor.insert("synthetic pending final text"));
        app.archive
            .set_sync_remote(temporary.0.join("missing.git").to_str().unwrap())
            .unwrap();
        assert!(update(&mut gui, Message::QuitRequested).units() > 0);
        assert_eq!(gui.quit_state, QuitState::Publishing);
        let err = stage_sync(path.clone()).unwrap_err().to_string();
        assert_eq!(update(&mut gui, Message::SyncFinished(Err(err))).units(), 0);
        assert!(matches!(gui.quit_state, QuitState::Failed(_)));
        assert!(gui.app.as_ref().unwrap().quit);
        assert!(!Archive::open(path).unwrap().is_dirty().unwrap());
        assert_eq!(update(&mut gui, Message::QuitRequested).units(), 0);
        // Only a distinct, explicit user action can now permit an offline exit.
        assert!(update(&mut gui, Message::QuitWithoutSync).units() > 0);
    }

    #[test]
    fn failed_background_push_retains_pending_and_retries_after_backoff() {
        let (mut gui, temporary) = fixture();
        gui.app.as_mut().unwrap().scheduler.sync_pending();
        let path = gui.app.as_ref().unwrap().archive.root().to_path_buf();
        gui.app
            .as_ref()
            .unwrap()
            .archive
            .set_sync_remote(temporary.0.join("missing.git").to_str().unwrap())
            .unwrap();
        assert!(service_tick(&mut gui, Instant::now()).units() > 0);
        let error = stage_sync(path.clone()).unwrap_err().to_string();
        assert_eq!(
            update(&mut gui, Message::SyncFinished(Err(error))).units(),
            0
        );
        assert!(!gui.sync_active);
        assert!(gui.sync_error.is_some());
        assert!(gui.app.as_ref().unwrap().scheduler.is_sync_pending());
        assert_eq!(service_tick(&mut gui, Instant::now()).units(), 0);
        gui.app
            .as_ref()
            .unwrap()
            .archive
            .set_sync_remote(temporary.0.join("remote.git").to_str().unwrap())
            .unwrap();
        let retry_at = gui.next_sync_at;
        assert!(service_tick(&mut gui, retry_at).units() > 0);
        let staged = stage_sync(path).unwrap();
        assert_eq!(
            update(&mut gui, Message::SyncFinished(Ok(staged))).units(),
            0
        );
        assert!(!gui.app.as_ref().unwrap().scheduler.is_sync_pending());
        assert!(gui.sync_error.is_none());
        assert_published(&gui, &temporary);
    }

    #[test]
    fn quit_during_prior_push_republishes_the_final_checkpoint() {
        let (mut gui, temporary) = fixture();
        let path = gui.app.as_ref().unwrap().archive.root().to_path_buf();
        assert!(service_tick(&mut gui, Instant::now()).units() > 0);
        let old_stage = stage_sync(path.clone()).unwrap();
        assert!(gui
            .app
            .as_mut()
            .unwrap()
            .editor
            .insert("newer text at Quit"));
        assert_eq!(update(&mut gui, Message::QuitRequested).units(), 0);
        assert_eq!(gui.quit_state, QuitState::WaitingForCurrentSync);
        assert!(gui.sync_active);
        // Finishing the older worker MUST schedule a new network operation.
        assert!(update(&mut gui, Message::SyncFinished(Ok(old_stage))).units() > 0);
        assert_eq!(gui.quit_state, QuitState::Publishing);
        let last_stage = stage_sync(path.clone()).unwrap();
        assert_eq!(last_stage.outcome(), SyncOutcome::Published);
        assert!(update(&mut gui, Message::SyncFinished(Ok(last_stage))).units() > 0);
        assert_published(&gui, &temporary);
        assert_eq!(
            git(&path, &["rev-parse", "refs/remotes/carta-sync/carta"]),
            git(&path, &["rev-parse", "HEAD"])
        );
    }

    #[test]
    fn failed_quit_can_retry_after_remote_is_repaired() {
        let (mut gui, temporary) = fixture();
        let path = gui.app.as_ref().unwrap().archive.root().to_path_buf();
        gui.app.as_mut().unwrap().editor.insert("last line");
        gui.app
            .as_ref()
            .unwrap()
            .archive
            .set_sync_remote(temporary.0.join("absent.git").to_str().unwrap())
            .unwrap();
        assert!(update(&mut gui, Message::QuitRequested).units() > 0);
        let err = stage_sync(path.clone()).unwrap_err().to_string();
        assert_eq!(update(&mut gui, Message::SyncFinished(Err(err))).units(), 0);
        assert!(matches!(gui.quit_state, QuitState::Failed(_)));

        gui.app
            .as_ref()
            .unwrap()
            .archive
            .set_sync_remote(temporary.0.join("remote.git").to_str().unwrap())
            .unwrap();
        assert!(update(&mut gui, Message::QuitRetry).units() > 0);
        assert_eq!(gui.quit_state, QuitState::Publishing);
        let staged = stage_sync(path).unwrap();
        assert!(update(&mut gui, Message::SyncFinished(Ok(staged))).units() > 0);
        assert_published(&gui, &temporary);
    }

    #[test]
    fn pending_checkpoint_without_remote_is_not_acknowledged() {
        let (mut gui, _temporary) = fixture();
        let app = gui.app.as_mut().unwrap();
        app.archive.clear_sync_remote().unwrap();
        app.scheduler.sync_pending();
        let generation = app.scheduler.sync_generation();
        assert_eq!(service_tick(&mut gui, Instant::now()).units(), 0);
        assert!(!gui.sync_active);
        assert!(gui.sync_error.is_none());
        let app = gui.app.as_ref().unwrap();
        assert!(app.scheduler.is_sync_pending());
        assert_eq!(app.scheduler.sync_generation(), generation);
    }

    #[test]
    fn background_sync_rejects_stale_archive_or_waits_for_dirty_editor() {
        for dirty_editor in [false, true] {
            let (mut gui, temporary) = fixture();
            assert!(gui
                .app
                .as_mut()
                .unwrap()
                .editor
                .insert("synthetic final text"));
            gui.app.as_mut().unwrap().autosave().unwrap();
            assert!(service_tick(&mut gui, Instant::now()).units() > 0);
            let path = gui.app.as_ref().unwrap().archive.root().to_path_buf();
            let staged = incoming_stage(&gui, &temporary);
            let app = gui.app.as_mut().unwrap();
            // Simulate an out-of-band change after the worker pinned its HEAD.
            if dirty_editor {
                assert!(app.editor.insert("synthetic unexpected edit"));
            } else {
                app.archive
                    .create_document("synthetic concurrent change")
                    .unwrap();
                app.archive
                    .checkpoint(
                        CheckpointKind::Structural,
                        Some("synthetic concurrent checkpoint"),
                    )
                    .unwrap();
            }
            let head = git(&path, &["rev-parse", "HEAD"]);
            let revision = app.editor.content_revision();
            assert_eq!(
                update(&mut gui, Message::SyncFinished(Ok(staged))).units(),
                0
            );
            assert!(!gui.sync_active);
            let app = gui.app.as_ref().unwrap();
            assert!(!app.quit);
            assert_eq!(app.editor.is_dirty(), dirty_editor);
            assert_eq!(app.editor.content_revision(), revision);
            assert_eq!(git(&path, &["rev-parse", "HEAD"]), head);
            assert_eq!(service_tick(&mut gui, Instant::now()).units(), 0);
        }
    }

    #[test]
    fn background_worker_does_not_freeze_paste_or_local_autosave() {
        let (mut gui, _temporary) = fixture();
        assert!(gui
            .app
            .as_mut()
            .unwrap()
            .editor
            .insert("synthetic final text"));
        assert!(service_tick(&mut gui, Instant::now()).units() > 0);
        let revision = gui.app.as_ref().unwrap().editor.content_revision();
        let _ = update(
            &mut gui,
            Message::PasteText(Some("editable during sync".into())),
        );
        assert!(gui.app.as_ref().unwrap().editor.content_revision() > revision);
        let _ = update(
            &mut gui,
            Message::Tick(Instant::now() + Duration::from_secs(600)),
        );
        let app = gui.app.as_ref().unwrap();
        assert!(!app.editor.is_dirty());
        assert!(!app.quit);
        assert!(gui.sync_active);
        assert!(gui.last_saved_session.is_some());
    }

    #[test]
    fn window_close_cancels_leap_and_mouse_drag_and_saves_session() {
        use iced::keyboard::{key::Code, key::Physical, Key, Location, Modifiers};
        let press = |code| {
            Message::Raw(Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: Key::Character("a".into()),
                modified_key: Key::Character("a".into()),
                physical_key: Physical::Code(code),
                location: Location::Standard,
                modifiers: Modifiers::empty(),
                text: Some("a".into()),
                repeat: false,
            }))
        };
        for active in [false, true] {
            let (mut gui, _temporary) = fixture();
            let _ = update(&mut gui, press(Code::ControlLeft));
            if active {
                let _ = update(&mut gui, press(Code::KeyA));
                assert!(matches!(
                    gui.app.as_ref().unwrap().mode,
                    AppMode::Leap { .. }
                ));
            }
            gui.pointer_down = true;
            let _ = update(&mut gui, Message::QuitRequested);
            assert!(!gui.pointer_down);
            let app = gui.app.as_ref().unwrap();
            assert!(matches!(app.mode, AppMode::Editing));
            assert!(app.quit);
            assert!(gui.last_saved_session.is_some());
        }
    }

    #[test]
    fn emergency_quit_during_background_sync_skips_checkpoint_and_session() {
        use iced::keyboard::{key::Code, key::Physical, Key, Location, Modifiers};
        let (mut gui, _temporary) = fixture();
        assert!(service_tick(&mut gui, Instant::now()).units() > 0);
        assert!(gui
            .app
            .as_mut()
            .unwrap()
            .editor
            .insert("unsaved emergency text"));
        let head = git(
            gui.app.as_ref().unwrap().archive.root(),
            &["rev-parse", "HEAD"],
        );
        for code in [Code::KeyG, Code::ControlRight, Code::KeyG] {
            let event = iced::keyboard::Event::KeyPressed {
                key: Key::Character("g".into()),
                modified_key: Key::Character("g".into()),
                physical_key: Physical::Code(code),
                location: Location::Standard,
                modifiers: Modifiers::empty(),
                text: Some("g".into()),
                repeat: false,
            };
            let task = update(&mut gui, Message::Raw(Event::Keyboard(event)));
            if gui.app.as_ref().unwrap().kill_switch_triggered() {
                assert!(task.units() > 0);
            }
        }
        assert!(gui.app.as_ref().unwrap().kill_switch_triggered());
        assert_eq!(
            git(
                gui.app.as_ref().unwrap().archive.root(),
                &["rev-parse", "HEAD"]
            ),
            head
        );
        assert!(gui.app.as_ref().unwrap().editor.is_dirty());
        assert!(gui.last_saved_session.is_none());
    }

    #[test]
    fn archive_without_any_remote_is_normal_local_only() {
        let (mut gui, _temporary) = fixture();
        let app = gui.app.as_mut().unwrap();
        let path = app.archive.root().to_path_buf();
        app.archive.clear_sync_remote().unwrap();
        git(&path, &["config", "--unset", "carta.sync-disabled"]);
        git(&path, &["remote", "remove", "origin"]);
        app.scheduler.sync_pending();
        assert_eq!(service_tick(&mut gui, Instant::now()).units(), 0);
        assert!(gui.sync_error.is_none());
        assert!(gui.app.as_ref().unwrap().scheduler.is_sync_pending());
        let _ = update(
            &mut gui,
            Message::PasteText(Some("local-only writing".into())),
        );
        assert!(gui.app.as_ref().unwrap().editor.is_dirty());
    }

    #[test]
    fn deferred_incoming_update_rechecks_configuration_before_application() {
        let (mut gui, temporary) = fixture();
        gui.app.as_mut().unwrap().scheduler.sync_pending();
        let path = gui.app.as_ref().unwrap().archive.root().to_path_buf();
        assert!(service_tick(&mut gui, Instant::now()).units() > 0);
        let staged = incoming_stage(&gui, &temporary);
        gui.last_user_input = Instant::now();
        assert_eq!(
            update(&mut gui, Message::SyncFinished(Ok(staged))).units(),
            0
        );
        assert!(gui.sync_ready.is_some());
        git(
            &path,
            &[
                "config",
                "remote.carta-sync.pushurl",
                "synthetic-new-destination",
            ],
        );
        let quiet_at = gui.last_user_input + Duration::from_secs(3);
        assert_eq!(service_tick(&mut gui, quiet_at).units(), 0);
        assert!(gui.sync_ready.is_none());
        assert!(gui.sync_error.is_some());
        assert!(gui.app.as_ref().unwrap().scheduler.is_sync_pending());
    }

    #[test]
    fn repeated_failures_have_capped_backoff() {
        let (mut gui, _temporary) = fixture();
        let now = Instant::now();
        for _ in 0..100 {
            sync_backoff(&mut gui, now);
            assert!(gui.next_sync_at > now);
            assert!(gui.next_sync_at <= now + Duration::from_secs(480));
        }
        assert_eq!(gui.next_sync_at, now + Duration::from_secs(480));
    }

    #[test]
    fn adoption_failure_opens_editor_and_retries_without_clearing_warning() {
        let (mut gui, temporary) = fixture();
        let path = gui.app.as_ref().unwrap().archive.root().to_path_buf();
        gui.app
            .as_ref()
            .unwrap()
            .archive
            .clear_sync_remote()
            .unwrap();
        git(&path, &["config", "--unset", "carta.sync-disabled"]);
        let lock = path.join(".git/config.lock");
        std::fs::write(&lock, "synthetic config lock").unwrap();
        gui.app = None;
        gui.install_archive(Archive::open(&path).unwrap());
        assert!(gui.error.is_none());
        assert!(gui.sync_error.is_some());
        assert!(gui
            .app
            .as_mut()
            .unwrap()
            .editor
            .insert("writing despite adoption failure"));
        let retry_at = gui.next_sync_at;
        assert_eq!(service_tick(&mut gui, Instant::now()).units(), 0);
        assert_eq!(service_tick(&mut gui, retry_at).units(), 0);
        assert!(gui.next_sync_at > retry_at);
        std::fs::remove_file(lock).unwrap();
        let retry_at = gui.next_sync_at;
        assert!(service_tick(&mut gui, retry_at).units() > 0);
        assert!(gui.sync_error.is_some());
        assert_eq!(
            gui.app
                .as_ref()
                .unwrap()
                .archive
                .sync_remote()
                .unwrap()
                .as_deref(),
            temporary.0.join("remote.git").to_str()
        );
    }

    #[test]
    fn conflict_retains_pending_and_backs_off_while_editor_remains_available() {
        let (mut gui, temporary) = fixture();
        let path = gui.app.as_ref().unwrap().archive.root().to_path_buf();
        stage_sync(path.clone()).unwrap();
        let mut peer = Archive::clone_sync_remote(
            temporary.0.join("remote.git").to_str().unwrap(),
            temporary.0.join("peer"),
        )
        .unwrap();
        let document = peer.documents().next().unwrap().id();
        peer.edit_document(document, "remote concurrent content")
            .unwrap();
        peer.checkpoint(CheckpointKind::Structural, None).unwrap();
        peer.sync().unwrap();
        let app = gui.app.as_mut().unwrap();
        app.archive
            .edit_document(document, "local concurrent content")
            .unwrap();
        app.archive
            .checkpoint(CheckpointKind::Structural, None)
            .unwrap();
        app.scheduler.sync_pending();
        assert!(service_tick(&mut gui, Instant::now()).units() > 0);
        let staged = stage_sync(path).unwrap();
        assert_eq!(staged.outcome(), SyncOutcome::Conflict);
        assert_eq!(
            update(&mut gui, Message::SyncFinished(Ok(staged))).units(),
            0
        );
        assert!(gui.app.as_ref().unwrap().scheduler.is_sync_pending());
        assert!(gui.sync_error.is_some());
        for _ in 0..10 {
            assert_eq!(service_tick(&mut gui, Instant::now()).units(), 0);
        }
        assert!(gui
            .app
            .as_mut()
            .unwrap()
            .editor
            .insert("writing through a conflict"));
        let retry_at = gui.next_sync_at;
        assert!(service_tick(&mut gui, retry_at).units() > 0);
        assert!(gui.sync_error.is_some());
    }

    #[test]
    fn outbound_success_while_dirty_or_in_palette_does_not_pause_periodic_sync() {
        for synced in [false, true] {
            for palette in [false, true] {
                let (mut gui, _temporary) = fixture();
                let path = gui.app.as_ref().unwrap().archive.root().to_path_buf();
                if synced {
                    stage_sync(path.clone()).unwrap();
                }
                gui.app.as_mut().unwrap().scheduler.sync_pending();
                assert!(service_tick(&mut gui, Instant::now()).units() > 0);
                let staged = stage_sync(path.clone()).unwrap();
                assert_eq!(
                    staged.outcome(),
                    if synced {
                        SyncOutcome::Synced
                    } else {
                        SyncOutcome::Published
                    }
                );
                let app = gui.app.as_mut().unwrap();
                assert!(app.editor.insert("continuous writing"));
                if palette {
                    app.open_palette();
                }
                let revision = app.editor.content_revision();
                let head = git(&path, &["rev-parse", "HEAD"]);
                gui.last_user_input = Instant::now();
                gui.pointer_down = true;
                gui.sync_error = Some("previous failure".into());
                gui.sync_failures = 2;
                assert_eq!(
                    update(&mut gui, Message::SyncFinished(Ok(staged))).units(),
                    0
                );
                assert!(gui.sync_ready.is_none());
                assert!(!gui.sync_active);
                assert!(gui.sync_error.is_none());
                assert_eq!(gui.sync_failures, 0);
                let app = gui.app.as_ref().unwrap();
                assert!(!app.scheduler.is_sync_pending());
                assert!(app.editor.is_dirty());
                assert_eq!(app.editor.content_revision(), revision);
                assert_eq!(git(&path, &["rev-parse", "HEAD"]), head);
                assert_eq!(matches!(app.mode, AppMode::Palette { .. }), palette);
                let periodic_at = gui.next_sync_at;
                gui.last_user_input = periodic_at;
                assert!(service_tick(&mut gui, periodic_at).units() > 0);
                assert!(gui.sync_active);
            }
        }
    }

    #[test]
    fn older_publication_keeps_new_checkpoint_queued_and_starts_next_job() {
        let (mut gui, _temporary) = fixture();
        let path = gui.app.as_ref().unwrap().archive.root().to_path_buf();
        gui.app.as_mut().unwrap().scheduler.sync_pending();
        assert!(service_tick(&mut gui, Instant::now()).units() > 0);
        let staged = stage_sync(path.clone()).unwrap();
        assert_eq!(staged.outcome(), SyncOutcome::Published);
        let started_generation = gui.sync_started_generation;
        let app = gui.app.as_mut().unwrap();
        app.archive
            .create_document("newer committed document")
            .unwrap();
        app.archive
            .checkpoint(CheckpointKind::Structural, None)
            .unwrap();
        app.scheduler.sync_pending();
        let newer_generation = app.scheduler.sync_generation();
        assert_ne!(newer_generation, started_generation);
        assert!(app.editor.insert("still typing"));
        app.open_palette();
        gui.last_user_input = Instant::now();
        assert!(update(&mut gui, Message::SyncFinished(Ok(staged))).units() > 0);
        assert!(gui.sync_ready.is_none());
        assert!(gui.sync_active);
        assert_eq!(gui.sync_started_generation, newer_generation);
        assert!(gui.app.as_ref().unwrap().scheduler.is_sync_pending());
        let newer_stage = stage_sync(path).unwrap();
        assert_eq!(
            update(&mut gui, Message::SyncFinished(Ok(newer_stage))).units(),
            0
        );
        assert!(!gui.app.as_ref().unwrap().scheduler.is_sync_pending());
        assert!(gui.sync_ready.is_none());
    }

    #[test]
    fn result_from_changed_configuration_never_acknowledges_pending() {
        for change in ["disabled", "push-url", "fetch-url", "rewrite"] {
            let (mut gui, temporary) = fixture();
            gui.app.as_mut().unwrap().scheduler.sync_pending();
            let path = gui.app.as_ref().unwrap().archive.root().to_path_buf();
            assert!(service_tick(&mut gui, Instant::now()).units() > 0);
            let staged = stage_sync(path.clone()).unwrap();
            if change == "push-url" {
                git(
                    &path,
                    &[
                        "config",
                        "remote.carta-sync.pushurl",
                        "synthetic-different-destination",
                    ],
                );
            } else if change == "disabled" {
                gui.app
                    .as_ref()
                    .unwrap()
                    .archive
                    .clear_sync_remote()
                    .unwrap();
            } else if change == "fetch-url" {
                gui.app
                    .as_ref()
                    .unwrap()
                    .archive
                    .set_sync_remote("synthetic-new-fetch-destination")
                    .unwrap();
            } else {
                git(
                    &path,
                    &[
                        "config",
                        "url.synthetic-new-destination.insteadOf",
                        temporary.0.join("remote.git").to_str().unwrap(),
                    ],
                );
            }
            assert_eq!(
                update(&mut gui, Message::SyncFinished(Ok(staged))).units(),
                0
            );
            assert!(gui.app.as_ref().unwrap().scheduler.is_sync_pending());
            assert!(gui.sync_error.is_some());
            assert_eq!(service_tick(&mut gui, Instant::now()).units(), 0);
            assert!(gui.app.as_mut().unwrap().editor.insert("still editable"));
        }
    }

    #[test]
    fn detached_worker_does_not_delay_runtime_shutdown() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (finished_tx, finished_rx) = std::sync::mpsc::channel();
        runtime.block_on(async {
            let task = tokio::spawn(detached_job(move || {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                finished_tx.send(()).unwrap();
                Ok(())
            }));
            tokio::task::yield_now().await;
            started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            task.abort();
        });
        let before = Instant::now();
        drop(runtime);
        assert!(before.elapsed() < Duration::from_secs(1));
        release_tx.send(()).unwrap();
        finished_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    }

    #[test]
    fn clone_requests_are_single_flight_and_close_does_not_wait() {
        let (mut gui, temporary) = fixture();
        gui.app = None;
        gui.setup_path = Some(temporary.0.join("clone"));
        gui.clone_url = temporary.0.join("remote.git").to_str().unwrap().into();
        assert!(update(&mut gui, Message::CloneDefaultArchive).units() > 0);
        assert!(gui.clone_active);
        assert_eq!(update(&mut gui, Message::CloneDefaultArchive).units(), 0);
        assert_eq!(update(&mut gui, Message::CreateDefaultArchive).units(), 0);
        assert!(update(&mut gui, Message::QuitRequested).units() > 0);
    }

    #[test]
    fn pasted_line_endings_are_normalized() {
        assert_eq!(normalize_clipboard("a\r\nb\rc\n"), "a\nb\nc\n");
    }
}
