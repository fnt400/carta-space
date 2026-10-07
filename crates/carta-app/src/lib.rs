//! Frontend-independent interactive application model for Carta Space.
//!
//! This crate owns reusable editing and application-state primitives. Platform
//! frontends translate native input into Carta semantics and render this state;
//! they must not grow second implementations of editing behavior.

pub mod editor;
pub mod help;
pub mod palette;
pub mod session;
pub mod view;

pub use editor::{CompositeEditor, Cursor, Region};
pub use help::HelpKind;
pub use session::{Position, SavedView, Session};
pub use view::{Scheduler, View};
