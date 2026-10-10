//! Translation of a TSF document change into one host [`ImeUpdate`].
//!
//! The host shows the document as last reported. Its replaceable range (the
//! "slot") is the preedit while composing, otherwise its selection. Every
//! input-method edit is expressed relative to that slot: delete around it,
//! commit a replacement, then show the composition after the commit.
use crate::{Error, ImeUpdate, Preedit};
use std::ops::Range;

/// What the host displays, in UTF-16 units of the TSF document.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Shown {
    pub text: Vec<u16>,
    /// The preedit while composing, otherwise the host's selection.
    pub slot: Range<usize>,
    pub composing: bool,
    /// Preedit caret endpoints, relative to the slot start.
    pub cursor: Option<(usize, usize)>,
}

/// The document after input-method edits.
pub(crate) struct Document<'a> {
    pub text: &'a [u16],
    /// A non-empty composition range.
    pub composition: Option<Range<usize>>,
    /// Anchor and active end.
    pub selection: (usize, usize),
}

fn utf8_len(units: &[u16]) -> usize {
    char::decode_utf16(units.iter().copied())
        .map(|c| c.map_or(3, char::len_utf8))
        .sum()
}

fn low_surrogate(units: &[u16], index: usize) -> bool {
    units
        .get(index)
        .is_some_and(|unit| (0xdc00..0xe000).contains(unit))
}

/// The update showing `now` and what the host then displays, or `None` when
/// the host already displays it (for example after a selection-only change).
/// Fails when the composition does not end the changed range, which the
/// host's edit model cannot express.
pub(crate) fn edit(shown: &Shown, now: &Document<'_>) -> Result<Option<(ImeUpdate, Shown)>, Error> {
    let (old, new) = (shown.text.as_slice(), now.text);
    let cursor = now.composition.as_ref().and_then(|c| {
        let (anchor, focus) = now.selection;
        let inside = |i: usize| (c.start..=c.end).contains(&i);
        (inside(anchor) && inside(focus)).then(|| {
            (
                utf8_len(&new[c.start..anchor]),
                utf8_len(&new[c.start..focus]),
            )
        })
    });
    if old == new
        && match &now.composition {
            None => !shown.composing,
            Some(c) => shown.composing && *c == shown.slot && cursor == shown.cursor,
        }
    {
        return Ok(None);
    }
    // The replaced range: the longest common prefix and suffix that leave
    // both the slot and the composition inside it. Capping, rather than
    // widening afterwards, keeps repeated text ("a|a" + "a") unambiguous.
    let (mut start_cap, mut end_cap) = (shown.slot.start, old.len() - shown.slot.end);
    if let Some(c) = &now.composition {
        start_cap = start_cap.min(c.start);
        end_cap = end_cap.min(new.len() - c.end);
    }
    let mut start = old
        .iter()
        .zip(new)
        .take(start_cap)
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(end_cap.min(old.len().min(new.len()) - start))
        .take_while(|(a, b)| a == b)
        .count();
    let (mut end, mut new_end) = (old.len() - suffix, new.len() - suffix);
    // Never split a surrogate pair that differs in its second unit. Valid
    // composition bounds never sit inside a pair, so this keeps them inside.
    if low_surrogate(old, start) {
        start -= 1;
    }
    if low_surrogate(old, end) {
        end += 1;
        new_end += 1;
    }
    if now.composition.as_ref().is_some_and(|c| c.end != new_end) {
        return Err(Error::InvalidIme(
            "input method changed text after its composition",
        ));
    }
    let commit_end = now.composition.as_ref().map_or(new_end, |c| c.start);
    let decode = |units: &[u16]| String::from_utf16(units).map_err(|_| Error::InvalidUtf16);
    let commit = decode(&new[start..commit_end])?;
    let preedit = decode(&new[commit_end..new_end])?;
    let update = ImeUpdate {
        delete_before: utf8_len(&old[start..shown.slot.start]),
        delete_after: utf8_len(&old[shown.slot.end..end]),
        // While composing, an empty commit leaves the slot to the preedit.
        commit: (now.composition.is_none() || !commit.is_empty()).then_some(commit),
        preedit: Preedit {
            text: preedit,
            cursor,
        },
    };
    let shown = Shown {
        text: new.to_vec(),
        slot: now.composition.clone().unwrap_or(new_end..new_end),
        composing: now.composition.is_some(),
        cursor,
    };
    Ok(Some((update, shown)))
}
