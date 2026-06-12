//! Facade crate for sushline's line-editing and history components.
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
