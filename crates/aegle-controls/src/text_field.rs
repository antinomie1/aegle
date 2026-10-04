use aegle_text::{Editor, HitSelection, Movement, Selection, TextError, TextSystem};

use crate::{Action, Capture, Input, Key, KeyInput, Outcome, PointerId, PointerKind};

/// Single- or multiline editing behavior backed by one retained [`Editor`].
///
/// The host supplies editor-local pointer coordinates (including scroll), owns
/// focus/capture and synchronizes native IME after applying the returned effects.
/// The editor's accumulated changes remain available through `editor_mut()`.
pub struct TextField {
    editor: Editor,
    enabled: bool,
    focused: bool,
    drag: Option<PointerId>,
}

impl TextField {
    /// Reuses an already initialized editor and its history/layout.
    pub fn new(editor: Editor) -> Self {
        Self {
            editor,
            enabled: true,
            focused: false,
            drag: None,
        }
    }

    /// Text, selection, paint and IME geometry all come from this editor.
    pub fn editor(&self) -> &Editor {
        &self.editor
    }
    /// Explicit application editing and invalidation access. Application setters
    /// must also resynchronize native IME and refresh host layout/semantics.
    pub fn editor_mut(&mut self) -> &mut Editor {
        &mut self.editor
    }
    /// Whether user interaction is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
    /// Whether the host assigned logical focus.
    pub fn is_focused(&self) -> bool {
        self.focused
    }
    /// Whether this field should have a native editable IME session.
    pub fn accepts_ime(&self) -> bool {
        self.enabled && self.focused && !self.editor.is_read_only()
    }

    /// Disabling ends capture and composition without replacing committed text.
    /// The host applies `reset_ime` and updates its focus policy.
    pub fn set_enabled(&mut self, fonts: &mut TextSystem, enabled: bool) -> Outcome {
        if self.enabled == enabled {
            return Outcome::default();
        }
        let was_ime = self.accepts_ime();
        self.enabled = enabled;
        let mut result = if enabled {
            Outcome::default()
        } else {
            self.cancel(fonts)
        };
        result.reset_ime |= was_ime && !enabled;
        result.repaint = true;
        result
    }

    /// Applies a default action after event listeners had an opportunity to
    /// prevent it. Read-only fields still allow navigation and selection.
    pub fn handle(
        &mut self,
        fonts: &mut TextSystem,
        input: Input<'_>,
    ) -> Result<Outcome, TextError> {
        match input {
            Input::Focus(focused) => {
                let changed = self.focused != focused;
                self.focused = focused;
                self.editor.break_undo_group();
                let mut result = if focused {
                    Outcome::default()
                } else {
                    self.cancel(fonts)
                };
                result.repaint |= changed;
                result.reset_ime |= changed && !focused;
                Ok(result)
            }
            Input::Cancel => Ok(self.cancel(fonts)),
            Input::Key(key) if self.enabled && self.focused && key.pressed => self.key(fonts, key),
            Input::Ime(edit) if self.accepts_ime() => {
                fonts.edit(&mut self.editor).apply_ime(edit)?;
                Ok(Outcome {
                    handled: true,
                    repaint: true,
                    ..Outcome::default()
                })
            }
            Input::Pointer(pointer) if self.enabled => {
                if !pointer.position.x.is_finite() || !pointer.position.y.is_finite() {
                    return Err(TextError::InvalidPosition);
                }
                let mut result = Outcome::default();
                match pointer.kind {
                    PointerKind::Down { clicks } if pointer.inside && self.drag.is_none() => {
                        result.reset_ime = fonts.edit(&mut self.editor).cancel_preedit();
                        self.editor.break_undo_group();
                        let hit = match clicks {
                            _ if pointer.modifiers.shift => HitSelection::Extend,
                            2 => HitSelection::Word,
                            3.. => HitSelection::Line,
                            _ => HitSelection::Caret,
                        };
                        fonts
                            .edit(&mut self.editor)
                            .select_at(pointer.position, hit)?;
                        self.drag = Some(pointer.id);
                        result.focus = true;
                        result.capture = Some(Capture::Acquire(pointer.id));
                        result.handled = true;
                        result.repaint = true;
                    }
                    PointerKind::Move | PointerKind::Up if self.drag == Some(pointer.id) => {
                        fonts
                            .edit(&mut self.editor)
                            .select_at(pointer.position, HitSelection::Extend)?;
                        if pointer.kind == PointerKind::Up {
                            self.drag = None;
                            result.capture = Some(Capture::Release(pointer.id));
                        }
                        result.handled = true;
                        result.repaint = true;
                    }
                    PointerKind::Cancel if self.drag == Some(pointer.id) => {
                        result = self.cancel(fonts)
                    }
                    _ => {}
                }
                Ok(result)
            }
            _ => Ok(Outcome::default()),
        }
    }

    fn cancel(&mut self, fonts: &mut TextSystem) -> Outcome {
        let drag = self.drag.take();
        let composing = fonts.edit(&mut self.editor).cancel_preedit();
        self.editor.break_undo_group();
        Outcome {
            handled: drag.is_some() || composing,
            repaint: composing,
            capture: drag.map(Capture::Release),
            reset_ime: composing,
            ..Outcome::default()
        }
    }

    fn key(&mut self, fonts: &mut TextSystem, key: KeyInput<'_>) -> Result<Outcome, TextError> {
        let command = if cfg!(target_os = "macos") {
            key.modifiers.meta
        } else {
            key.modifiers.control
        };
        let word = if cfg!(target_os = "macos") {
            key.modifiers.alt
        } else {
            key.modifiers.control
        };
        let movement = match key.key {
            Key::Left if word => Some(Movement::WordLeft),
            Key::Right if word => Some(Movement::WordRight),
            Key::Left => Some(Movement::Left),
            Key::Right => Some(Movement::Right),
            Key::Up => Some(Movement::Up),
            Key::Down => Some(Movement::Down),
            Key::Home if command => Some(Movement::TextStart),
            Key::End if command => Some(Movement::TextEnd),
            Key::Home => Some(Movement::LineStart),
            Key::End => Some(Movement::LineEnd),
            _ => None,
        };
        let select_all = command && matches!(key.key, Key::Character('a' | 'A'));
        let undo = command && matches!(key.key, Key::Character('z' | 'Z'));
        let redo = command && matches!(key.key, Key::Character('y' | 'Y'));
        let plain_text = !key.modifiers.control
            && !key.modifiers.meta
            && (!key.modifiers.alt || cfg!(target_os = "macos"))
            && !key.text.is_empty()
            && !key.text.chars().any(char::is_control);
        let edit_key = matches!(key.key, Key::Backspace | Key::Delete | Key::Enter)
            || undo
            || redo
            || plain_text;
        if movement.is_none() && !select_all && key.key != Key::Escape && !edit_key {
            return Ok(Outcome::default());
        }
        if self.editor.is_read_only() && edit_key && movement.is_none() && !select_all {
            return Ok(Outcome::default());
        }
        let mut result = Outcome {
            handled: true,
            repaint: true,
            ..Outcome::default()
        };
        result.reset_ime = fonts.edit(&mut self.editor).cancel_preedit();
        if let Some(movement) = movement {
            fonts
                .edit(&mut self.editor)
                .move_cursor(movement, key.modifiers.shift)?;
        } else if select_all {
            let len = self.editor.text().len();
            fonts.edit(&mut self.editor).select(Selection {
                anchor: 0,
                focus: len,
            })?;
        } else if undo || redo {
            let mut driver = fonts.edit(&mut self.editor);
            if redo || key.modifiers.shift {
                driver.redo()?;
            } else {
                driver.undo()?;
            }
        } else {
            match key.key {
                Key::Escape => {}
                Key::Backspace => {
                    fonts.edit(&mut self.editor).backspace()?;
                }
                Key::Delete => {
                    fonts.edit(&mut self.editor).delete()?;
                }
                Key::Enter if self.editor.is_multiline() => {
                    fonts.edit(&mut self.editor).insert("\n")?;
                }
                Key::Enter => {
                    if !key.repeat {
                        result.action = Some(Action::Submit);
                    }
                }
                _ if plain_text => {
                    fonts.edit(&mut self.editor).insert(key.text)?;
                }
                _ => {}
            }
        }
        Ok(result)
    }
}
