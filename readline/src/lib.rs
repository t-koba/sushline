//! Pure Rust line-editing foundation for interactive command interpreters.

mod bind;
mod buffer;
mod command;
mod completion;
mod config;
mod display;
mod editor;
mod hooks;
mod input;
mod inputrc;
mod keymap;
mod prompt;
mod state;
mod terminal;
mod variables;

pub use bind::{BindApi, BindError, BindQuery};
pub use buffer::{LineBuffer, WordStyle};
pub use completion::{
    CompletionAction, CompletionCandidate, CompletionContext, CompletionOptions, CompletionRequest,
    CompletionResponse, CompletionType,
};
pub use config::{Config, EditingMode, InputrcPath};
pub use editor::{Editor, ReadlineError, ReadlineResult};
pub use history::expansion::{
    HistoryChars, HistoryExpansionError, HistoryExpansionPolicy, expand_history,
};
pub use history::{History, HistoryDirection, HistoryEntry, HistorySearchMatch, HistoryState};
pub use hooks::{CommandContext, Edit, HistoryExpansionContext, Hooks};
pub use inputrc::{InputrcError, InputrcParser};
pub use keymap::KeyMapName;
pub use prompt::Prompt;
pub use terminal::{Terminal, TerminalEvent, TerminalIo, TerminalSize};
pub use variables::Variables;
