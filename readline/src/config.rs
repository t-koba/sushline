use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditingMode {
    Emacs,
    Vi,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputrcPath {
    Discover,
    Disabled,
    Path(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub application_name: String,
    pub editing_mode: EditingMode,
    pub inputrc_path: InputrcPath,
    pub keyseq_timeout_ms: u64,
    pub auto_add_history: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            application_name: "sushline".to_string(),
            editing_mode: EditingMode::Emacs,
            inputrc_path: InputrcPath::Discover,
            keyseq_timeout_ms: 500,
            auto_add_history: false,
        }
    }
}
