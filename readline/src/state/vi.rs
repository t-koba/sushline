#[derive(Debug, Clone, Copy)]
pub(crate) enum ViMarkAction {
    Set,
    Goto,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum CharSearchMode {
    Forward,
    Backward,
    TillForward,
    TillBackward,
}

impl CharSearchMode {
    pub(crate) fn reversed(self) -> Self {
        match self {
            Self::Forward => Self::Backward,
            Self::Backward => Self::Forward,
            Self::TillForward => Self::TillBackward,
            Self::TillBackward => Self::TillForward,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViOperator {
    Change,
    Delete,
    Yank,
}
