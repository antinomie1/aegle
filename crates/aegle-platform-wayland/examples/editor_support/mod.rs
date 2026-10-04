//! Example-only glue. The platform crate does not depend on an editor or renderer.

use aegle_platform_wayland::{
    ImeCause, ImeHints, ImeRequest, ImeUpdate, KeyEvent, Keysym, Modifiers,
};
use aegle_text::{Editor, Movement, Selection, TextSystem};
use aegle_types::{Point, Rect};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn key(
    fonts: &mut TextSystem,
    editor: &mut Editor,
    key: KeyEvent,
    modifiers: Modifiers,
) -> Result<bool> {
    let movement = match key.keysym {
        Keysym::Left if modifiers.ctrl => Some(Movement::WordLeft),
        Keysym::Right if modifiers.ctrl => Some(Movement::WordRight),
        Keysym::Left => Some(Movement::Left),
        Keysym::Right => Some(Movement::Right),
        Keysym::Up => Some(Movement::Up),
        Keysym::Down => Some(Movement::Down),
        Keysym::Home if modifiers.ctrl => Some(Movement::TextStart),
        Keysym::End if modifiers.ctrl => Some(Movement::TextEnd),
        Keysym::Home => Some(Movement::LineStart),
        Keysym::End => Some(Movement::LineEnd),
        _ => None,
    };
    let length = editor.text().len();
    let mut driver = fonts.edit(editor);
    // The compositor forwards keys the IME did not consume. A manual editing
    // action cancels the old composition; modifier-only presses do not.
    if let Some(movement) = movement {
        driver.cancel_preedit();
        driver.move_cursor(movement, modifiers.shift)?;
    } else if modifiers.ctrl && matches!(key.keysym, Keysym::a | Keysym::A) {
        driver.cancel_preedit();
        driver.select(Selection {
            anchor: 0,
            focus: length,
        })?;
    } else if modifiers.ctrl && matches!(key.keysym, Keysym::z | Keysym::Z) {
        driver.cancel_preedit();
        if modifiers.shift {
            driver.redo()?;
        } else {
            driver.undo()?;
        }
    } else if modifiers.ctrl && matches!(key.keysym, Keysym::y | Keysym::Y) {
        driver.cancel_preedit();
        driver.redo()?;
    } else {
        match key.keysym {
            Keysym::BackSpace => {
                driver.cancel_preedit();
                driver.backspace()?;
            }
            Keysym::Delete => {
                driver.cancel_preedit();
                driver.delete()?;
            }
            Keysym::Escape => {
                driver.cancel_preedit();
            }
            Keysym::Return | Keysym::KP_Enter => {
                driver.cancel_preedit();
                driver.insert("\n")?;
            }
            _ if !modifiers.ctrl && !modifiers.alt && !modifiers.logo => {
                if let Some(text) = key.utf8.filter(|text| !text.chars().any(char::is_control)) {
                    if !text.is_empty() {
                        driver.cancel_preedit();
                        driver.insert(&text)?;
                        return Ok(true);
                    }
                }
                return Ok(false);
            }
            _ => return Ok(false),
        }
    }
    Ok(true)
}

pub fn apply_ime(fonts: &mut TextSystem, editor: &mut Editor, update: ImeUpdate) -> Result<()> {
    // A protocol reset removes the old preedit to a cursor. Unlike an explicit
    // application cancellation, it must not restore text replaced by composition.
    let commit = update.commit.as_deref().or_else(|| {
        (editor.composition_range().is_some() && update.preedit.text.is_empty()).then_some("")
    });
    if update.delete_before != 0 || update.delete_after != 0 {
        // Native deletion lengths exclude the selected/preedit range. Remove
        // the surrounding sides without accidentally counting selection bytes.
        fonts.edit(editor).cancel_preedit();
        let selection = editor.selection();
        let selected = selection.range();
        let start = selected
            .start
            .checked_sub(update.delete_before as usize)
            .ok_or("IME deletion precedes the document")?;
        let end = selected
            .end
            .checked_add(update.delete_after as usize)
            .filter(|end| editor.display_text().is_char_boundary(*end))
            .ok_or("IME deletion exceeds the document or splits UTF-8")?;
        if !editor.display_text().is_char_boundary(start) {
            return Err("IME deletion splits UTF-8".into());
        }
        if let Some(commit) = commit {
            // One exact replacement keeps delete + commit as one undo operation.
            fonts.edit(editor).replace(start..end, commit)?;
        } else {
            let retained_selection = editor.selected_text().to_owned();
            let mut driver = fonts.edit(editor);
            driver.replace(start..end, &retained_selection)?;
            driver.select(Selection {
                anchor: selection.anchor - update.delete_before as usize,
                focus: selection.focus - update.delete_before as usize,
            })?;
        }
    } else if let Some(commit) = commit {
        fonts.edit(editor).commit(commit)?;
    }
    // Keeping the composition alive between preedit-only events preserves the
    // original selected fragment and its single eventual undo operation.
    fonts.edit(editor).set_preedit(
        &update.preedit.text,
        update
            .preedit
            .cursor
            .map(|(anchor, focus)| Selection { anchor, focus }),
    )?;
    Ok(())
}

pub fn request(editor: &Editor, origin: Point, scroll: f32, cause: ImeCause) -> Result<ImeRequest> {
    let display = editor.display_text();
    let (prefix, suffix, cursor, anchor) = if let Some(range) = editor.composition_range() {
        // Preedit replaces its original selection in the protocol's surrounding
        // view. Editor::text() intentionally preserves that selection for undo.
        (
            &display[..range.start],
            &display[range.end..],
            range.start,
            range.start,
        )
    } else {
        let selection = editor.selection();
        (display, "", selection.focus, selection.anchor)
    };
    let low = cursor.min(anchor);
    let high = cursor.max(anchor);
    if high - low > 4000 {
        return Err("example IME selection exceeds the 4000-byte protocol excerpt".into());
    }
    let length = prefix.len() + suffix.len();
    let spare = 4000 - (high - low);
    let mut start = low.saturating_sub(spare / 2);
    let mut end = (start + 4000).min(length);
    start = end.saturating_sub(4000).min(start);
    let boundary = |index: usize| {
        if index <= prefix.len() {
            prefix.is_char_boundary(index)
        } else {
            suffix.is_char_boundary(index - prefix.len())
        }
    };
    while !boundary(start) {
        start += 1;
    }
    while !boundary(end) {
        end -= 1;
    }
    let mut surrounding = String::with_capacity(end - start);
    if start < prefix.len() {
        surrounding.push_str(&prefix[start..end.min(prefix.len())]);
    }
    if end > prefix.len() {
        surrounding.push_str(&suffix[start.saturating_sub(prefix.len())..end - prefix.len()]);
    }
    let Rect {
        origin: mut caret,
        size,
    } = editor.ime_rect();
    caret.x += origin.x;
    caret.y += origin.y - scroll;
    Ok(ImeRequest {
        surrounding,
        cursor: cursor - start,
        anchor: anchor - start,
        cursor_rect: Rect {
            origin: caret,
            size,
        },
        hints: ImeHints::Multiline,
        cause,
        ..Default::default()
    })
}
