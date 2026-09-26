use std::env;
use std::io::{self, stdout, Stdout, Write};
use std::time::Instant;

use crossterm::event::{
    read, Event, KeyCode, KeyEvent, KeyEventKind, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement};

const ENHANCEMENTS: KeyboardEnhancementFlags = KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
    .union(KeyboardEnhancementFlags::REPORT_EVENT_TYPES)
    .union(KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS)
    .union(KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES);

struct TerminalGuard {
    stdout: Stdout,
    raw_mode: bool,
    enhancements: bool,
}

impl TerminalGuard {
    fn enter() -> io::Result<(Self, Option<io::Error>)> {
        enable_raw_mode()?;

        let mut guard = Self {
            stdout: stdout(),
            raw_mode: true,
            enhancements: false,
        };

        let detection_error = match supports_keyboard_enhancement() {
            Ok(true) => {
                execute!(guard.stdout, PushKeyboardEnhancementFlags(ENHANCEMENTS))?;
                guard.enhancements = true;
                None
            }
            Ok(false) => None,
            Err(error) => Some(error),
        };

        Ok((guard, detection_error))
    }

    fn line(&mut self, message: impl std::fmt::Display) -> io::Result<()> {
        write!(self.stdout, "{message}\r\n")?;
        self.stdout.flush()
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.enhancements {
            let _ = execute!(self.stdout, PopKeyboardEnhancementFlags);
        }
        if self.raw_mode {
            let _ = disable_raw_mode();
        }
        let _ = self.stdout.flush();
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("keyboard-events: {error}");
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    let (mut terminal, detection_error) = TerminalGuard::enter()?;

    terminal.line("Crossterm enhanced keyboard event probe")?;
    terminal.line(format!(
        "TERM={:?} TERM_PROGRAM={:?} COLORTERM={:?} SSH_CONNECTION={}",
        env::var("TERM").ok(),
        env::var("TERM_PROGRAM").ok(),
        env::var("COLORTERM").ok(),
        if env::var_os("SSH_CONNECTION").is_some() {
            "set"
        } else {
            "not set"
        }
    ))?;

    if terminal.enhancements {
        terminal.line(format!(
            "enhanced keyboard reporting: SUPPORTED and ENABLED ({ENHANCEMENTS:?})"
        ))?;
    } else if let Some(error) = detection_error {
        terminal.line(format!(
            "enhanced keyboard reporting: DETECTION ERROR ({error}); using legacy events"
        ))?;
    } else {
        terminal.line("enhanced keyboard reporting: NOT SUPPORTED; using legacy events")?;
    }

    terminal
        .line("Expected distinguishing codes: Modifier(LeftControl) and Modifier(RightControl).")?;
    terminal.line("Exit with Esc or F12 (on Press).")?;
    terminal.line("")?;

    let started = Instant::now();
    let mut sequence = 0_u64;

    loop {
        let event = read()?;
        sequence += 1;

        match event {
            Event::Key(key) => {
                terminal.line(format!(
                    "{sequence:06} +{:>10.3?} KEY kind={:<7} code={:?} modifiers={:?} state={:?}",
                    started.elapsed(),
                    kind_name(key.kind),
                    key.code,
                    key.modifiers,
                    key.state
                ))?;

                if should_exit(key) {
                    break;
                }
            }
            other => terminal.line(format!(
                "{sequence:06} +{:>10.3?} NON-KEY {other:?}",
                started.elapsed()
            ))?,
        }
    }

    terminal.line("Restoring terminal state...")?;
    Ok(())
}

fn kind_name(kind: KeyEventKind) -> &'static str {
    match kind {
        KeyEventKind::Press => "Press",
        KeyEventKind::Repeat => "Repeat",
        KeyEventKind::Release => "Release",
    }
}

fn should_exit(key: KeyEvent) -> bool {
    key.kind == KeyEventKind::Press && matches!(key.code, KeyCode::Esc | KeyCode::F(12))
}
