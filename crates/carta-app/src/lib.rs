//! Frontend-independent interactive application model for Carta Space.
//!
//! This crate owns reusable editing and application-state primitives. Platform
//! frontends translate native input into Carta semantics and render this state;
//! they must not grow second implementations of editing behavior.

pub mod action;
pub mod application;
pub mod editor;
pub mod dictionary_manager;
pub mod help;
pub mod mode;
pub mod palette;
pub mod session;
pub mod spelling;
pub mod status;
pub mod view;

pub use action::Action;
pub use application::{App, AppResult, Command};
pub use editor::{CompositeEditor, Cursor, Region};
pub use help::HelpKind;
pub use mode::{AppMode, Choice, ConfirmAction, ModeAction, PromptAction, ResultRow, SelectAction};
pub use session::{Position, SavedView, Session};
pub use status::StatusBar;
pub use view::{Scheduler, View};
