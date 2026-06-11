use super::{EditorState, repeat};

#[derive(Debug, Default)]
pub(crate) struct KillRingState {
    pub(crate) kill_ring: Vec<Vec<u8>>,
    pub(crate) last_was_kill: bool,
    pub(crate) last_yank: Option<YankState>,
}

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

impl EditorState {
    pub(crate) fn push_kill(&mut self, text: impl Into<Vec<u8>>, direction: KillDirection) {
        let text = text.into();
        self.kill.last_yank = None;
        if text.is_empty() {
            self.kill.last_was_kill = true;
            return;
        }
        self.store_active_vi_register(&text);

        if self.kill.last_was_kill
            && let Some(last) = self.kill.kill_ring.last_mut()
        {
            match direction {
                KillDirection::Forward => last.extend_from_slice(&text),
                KillDirection::Backward => {
                    last.splice(0..0, text.iter().copied());
                }
            }
        } else {
            self.kill.kill_ring.push(text);
        }
        self.kill.last_was_kill = true;
    }

    pub(crate) fn yank(&mut self) {
        self.kill.last_was_kill = false;
        let Some(index) = self.kill.kill_ring.len().checked_sub(1) else {
            self.kill.last_yank = None;
            return;
        };
        self.yank_from_index(index);
    }

    pub(crate) fn yank_pop(&mut self) {
        self.kill.last_was_kill = false;
        let Some(last_yank) = self.kill.last_yank else {
            return;
        };
        if self.kill.kill_ring.is_empty() {
            self.kill.last_yank = None;
            return;
        }

        let next_index = if last_yank.kill_index == 0 {
            self.kill.kill_ring.len() - 1
        } else {
            last_yank.kill_index - 1
        };
        let text = self.kill.kill_ring[next_index].clone();
        self.buffer
            .replace_range_bytes(last_yank.start, last_yank.end, &text);
        self.kill.last_yank = Some(YankState {
            start: last_yank.start,
            end: last_yank.start + text.len(),
            kill_index: next_index,
        });
    }

    pub(crate) fn yank_from_index(&mut self, index: usize) {
        let text = self.kill.kill_ring[index].clone();
        let start = self.buffer.point();
        repeat(self, |state| {
            state.buffer.insert_bytes(&text);
        });
        self.kill.last_yank = Some(YankState {
            start,
            end: self.buffer.point(),
            kill_index: index,
        });
    }

    pub(crate) fn vi_put(&mut self) {
        self.kill.last_was_kill = false;
        if let Some(register) = self.vi.active_vi_register.take()
            && let Some(text) = self
                .vi
                .vi_registers
                .get(&register.to_ascii_lowercase())
                .cloned()
        {
            repeat(self, |state| {
                state.buffer.insert_bytes(&text);
            });
            self.kill.last_yank = None;
            return;
        }
        self.yank();
    }

    pub(crate) fn vi_put_before(&mut self) {
        self.kill.last_was_kill = false;
        if let Some(register) = self.vi.active_vi_register.take()
            && let Some(text) = self
                .vi
                .vi_registers
                .get(&register.to_ascii_lowercase())
                .cloned()
        {
            repeat(self, |state| {
                state.buffer.insert_bytes(&text);
            });
            self.kill.last_yank = None;
            return;
        }
        let Some(index) = self.kill.kill_ring.len().checked_sub(1) else {
            self.kill.last_yank = None;
            return;
        };
        self.yank_from_index(index);
    }
}
