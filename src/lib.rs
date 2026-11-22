//! Facade crate for sushline's line-editing and history components.
//!
//! The main embedding example is compiled here; it is not run because it
//! needs a live terminal:
//!
//! ```no_run
//! use sushline::readline::{Config, Editor, History, Prompt, Terminal};
//!
//! let mut editor = Editor::new(Config::default(), Terminal::new(), History::new());
//! let _ = editor.read_line(Prompt::new("> "), &mut ());
//! ```
#![warn(missing_docs)]

/// Thin alias for the history crate.
///
/// Embedders normally use `sushline::readline::History` when working with an
/// `Editor`; this module remains available for direct history APIs.
pub mod history {
    pub use history::*;
}

/// Public readline API, including `Editor`, `History`, completion types, and
/// embedding hooks.
pub mod readline {
    pub use readline::*;
}
