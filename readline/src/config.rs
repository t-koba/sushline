use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// EditingMode.
pub enum EditingMode {
    /// Emacs.
    Emacs,
    /// Vi.
    Vi,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// InputrcPath.
pub enum InputrcPath {
    /// Discover.
    Discover,
    /// Disabled.
    Disabled,
    /// Path.
    Path(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Config.
pub struct Config {
    /// Application name.
    pub application_name: String,
    /// Editing mode.
    pub editing_mode: EditingMode,
    /// Inputrc path.
    pub inputrc_path: InputrcPath,
    /// Maximum nested `$include` depth for file and standalone inputrc parsing.
    ///
    /// Defaults to 32. The root input is depth 0; setting this to 0 rejects
    /// all includes. This is a Sushline-only recursion safety budget.
    pub inputrc_max_include_depth: usize,
    /// Keyseq timeout ms.
    pub keyseq_timeout_ms: u64,
    /// Auto add history.
    pub auto_add_history: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            application_name: "sushline".to_string(),
            editing_mode: EditingMode::Emacs,
            inputrc_path: InputrcPath::Discover,
            inputrc_max_include_depth: 32,
            keyseq_timeout_ms: 500,
            auto_add_history: false,
        }
    }
}
