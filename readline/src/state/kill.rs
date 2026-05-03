#[derive(Debug, Clone, Copy)]
pub(crate) struct YankState {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) kill_index: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct LastYankArgState {
    pub(crate) history_index: usize,
    pub(crate) arg: i32,
    pub(crate) range: Option<(usize, usize)>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum KillDirection {
    Forward,
    Backward,
}
