use crate::editor::{HistoryStats, Selection};
use std::{collections::VecDeque, mem::size_of};

/// A committed replacement; offsets and selections refer to their corresponding
/// before/after text. Preedit changes never enter the history individually.
pub(crate) struct Edit {
    pub start: usize,
    pub removed: String,
    pub inserted: String,
    pub before: Selection,
    pub after: Selection,
}

impl Edit {
    fn bytes(&self) -> usize {
        size_of::<Self>() + self.removed.capacity() + self.inserted.capacity()
    }

    fn is_typing(&self) -> bool {
        self.removed.is_empty()
            && !self.inserted.is_empty()
            && self.before.anchor == self.start
            && self.before.focus == self.start
            && self.after.anchor == self.start + self.inserted.len()
            && self.after.focus == self.after.anchor
    }
}

/// Bounded replacement deltas. `cursor` counts applied entries; entries after it
/// form the redo chain. The budget counts occupied entries and string capacity,
/// excluding spare deque slots and allocator overhead.
pub(crate) struct History {
    entries: VecDeque<Edit>,
    cursor: usize,
    bytes: usize,
    limit: usize,
    typing: bool,
}

impl History {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            cursor: 0,
            bytes: 0,
            limit,
            typing: false,
        }
    }

    pub(crate) fn record(&mut self, edit: Edit, merge_typing: bool) {
        // An unrecordable change invalidates the prior timeline, including redo.
        if edit.bytes() > self.limit {
            self.clear();
            return;
        }
        while self.entries.len() > self.cursor {
            self.bytes -= self.entries.pop_back().unwrap().bytes();
        }

        let typing = merge_typing && edit.is_typing();
        if self.typing && typing {
            let previous = self.entries.back_mut().unwrap();
            if previous.after == edit.before {
                let old_bytes = previous.bytes();
                previous.inserted.push_str(&edit.inserted);
                previous.after = edit.after;
                self.bytes = self.bytes - old_bytes + previous.bytes();
                self.trim();
                self.typing = !self.entries.is_empty();
                return;
            }
        }

        self.bytes += edit.bytes();
        self.entries.push_back(edit);
        self.cursor += 1;
        self.trim();
        self.typing = typing && !self.entries.is_empty();
    }

    // Check before copying a potentially large selection. Disabled/undersized
    // history never needs to allocate a discarded text snapshot.
    pub(crate) fn prepare(&mut self, removed: usize, inserted: usize) -> bool {
        let fits = removed
            .checked_add(inserted)
            .and_then(|bytes| bytes.checked_add(size_of::<Edit>()))
            .is_some_and(|bytes| bytes <= self.limit);
        if !fits {
            self.clear();
        }
        fits
    }

    pub(crate) fn break_group(&mut self) {
        self.typing = false;
    }

    pub(crate) fn undo(&mut self) -> Option<&Edit> {
        self.break_group();
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        self.entries.get(self.cursor)
    }

    pub(crate) fn redo(&mut self) -> Option<&Edit> {
        self.break_group();
        let edit = self.entries.get(self.cursor)?;
        self.cursor += 1;
        Some(edit)
    }

    pub(crate) fn clear(&mut self) {
        self.entries = VecDeque::new();
        self.cursor = 0;
        self.bytes = 0;
        self.typing = false;
    }

    pub(crate) fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
        self.break_group();
        if limit == 0 {
            self.clear();
        } else {
            self.trim();
            self.entries.shrink_to_fit();
        }
    }

    pub(crate) fn stats(&self) -> HistoryStats {
        HistoryStats {
            bytes: self.bytes,
            undo_steps: self.cursor,
            redo_steps: self.entries.len() - self.cursor,
        }
    }

    fn trim(&mut self) {
        while self.bytes > self.limit {
            let discarded = if self.cursor > 0 {
                self.cursor -= 1;
                self.entries.pop_front().unwrap()
            } else {
                // Keep the next redo step: subsequent steps depend on it.
                self.entries.pop_back().unwrap()
            };
            self.bytes -= discarded.bytes();
        }
    }
}
