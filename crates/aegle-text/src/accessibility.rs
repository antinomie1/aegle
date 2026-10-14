use accesskit::{Node, NodeId, Role, TextPosition, TextSelection, TreeUpdate};
use aegle_types::{Color, Point};
use parley::Cursor;

use crate::{EditorDriver, TextError, document::Caret, nav::Selected};

impl EditorDriver<'_> {
    /// Append text layout nodes and update their editable parent.
    ///
    /// The parent receives the editor's role, read-only state, text selection and
    /// `SetTextSelection` action. Its children/value/selection are replaced; the
    /// caller supplies its ID, name, bounds, focus and other control actions.
    /// Newly allocated IDs must be unique in the host's complete semantic tree;
    /// keep that ID namespace stable for the lifetime of this editor.
    /// `origin` is the text origin in the parent's coordinate space; apply the
    /// same scrolling/presentation transform as painting and hit testing.
    ///
    /// Runs describe [`crate::Editor::display_text`], including IME preedit and
    /// its underline. They must not be treated as committed application values.
    /// The committed value remains [`crate::Editor::text`]. Password editors
    /// expose the `PasswordInput` role and only their masked display runs.
    ///
    /// Parley retains run identities per hard line; Aegle additionally retains
    /// only each ID, its character count and line for checked actions.
    /// Emitting nodes allocates their text/geometry properties, so call only
    /// for requested semantic updates. The platform adapter owns the emitted
    /// snapshot.
    pub fn accessibility(
        &mut self,
        update: &mut TreeUpdate,
        node: &mut Node,
        mut next_node_id: impl FnMut() -> NodeId,
        origin: Point,
    ) -> Result<(), TextError> {
        self.ready()?;
        if !origin.x.is_finite() || !origin.y.is_finite() {
            return Err(TextError::InvalidPosition);
        }
        node.set_role(if self.editor.is_password() {
            Role::PasswordInput
        } else if self.editor.multiline {
            Role::MultilineTextInput
        } else {
            Role::TextInput
        });
        if self.editor.read_only {
            node.set_read_only();
        } else {
            node.clear_read_only();
        }
        // AccessKit 0.24 keeps a cleared property slot as None; push_child only
        // appends to a vector slot. Install an empty vector before Parley appends.
        node.set_children(Vec::new());
        node.clear_value();
        node.clear_text_selection();
        let editor = &mut *self.editor;
        let foreground = color(editor.doc.color);
        editor.access_runs.clear();
        let doc = &mut editor.doc;
        for (index, block) in doc.blocks.iter_mut().enumerate() {
            let start = update.nodes.len();
            block.access.build_nodes(
                &doc.text[block.shaped()],
                &block.layout,
                update,
                node,
                &mut next_node_id,
                f64::from(origin.x + block.origin.x),
                f64::from(origin.y + block.origin.y),
                |node, style| {
                    node.set_foreground_color(foreground);
                    if style.underline.is_some() {
                        node.set_underline(accesskit::TextDecoration {
                            style: accesskit::TextDecorationStyle::Solid,
                            color: foreground,
                        });
                    }
                },
            );
            editor.access_runs.extend(
                update.nodes[start..]
                    .iter()
                    .map(|(id, node)| (*id, node.character_lengths().len(), index)),
            );
        }
        let position = |caret: Caret| {
            let (index, cursor) = editor.doc.cursor(caret);
            let block = &editor.doc.blocks[index];
            cursor.to_access_position(&block.layout, &block.access)
        };
        if editor.show_cursor
            && let (Some(anchor), Some(focus)) =
                (position(editor.sel.anchor), position(editor.sel.focus))
        {
            node.set_text_selection(TextSelection { anchor, focus });
        }
        node.add_action(accesskit::Action::SetTextSelection);
        Ok(())
    }

    /// Apply a selection from this editor's most recently emitted text runs.
    ///
    /// AccessKit positions count selectable shaping clusters within a run, not
    /// UTF-8 bytes, Unicode scalars or UTF-16 units. The platform adapter converts
    /// native positions using the emitted character lengths; Parley converts the
    /// validated positions back into the existing editor's selection.
    ///
    /// Read-only editors still permit selection. Unknown IDs, invalid character
    /// indices and actions after a layout change but before a new semantic update
    /// return [`TextError::InvalidRange`] without changing state. Active preedit
    /// returns [`TextError::CompositionActive`]: the host must explicitly cancel
    /// its IME session and publish the restored text before accepting selections.
    pub fn select_accessibility(&mut self, selection: &TextSelection) -> Result<(), TextError> {
        self.no_composition()?;
        self.ready()?;
        let caret = |position: TextPosition| -> Result<Caret, TextError> {
            let (index, position) = normalize(position, &self.editor.access_runs)?;
            let doc = &self.editor.doc;
            let block = &doc.blocks[index];
            let cursor = Cursor::from_access_position(&position, &block.layout, &block.access)
                .ok_or(TextError::InvalidRange)?;
            Ok(doc.caret(index, cursor))
        };
        let (anchor, focus) = (caret(selection.anchor)?, caret(selection.focus)?);
        self.moved(Selected {
            anchor,
            focus,
            ..Selected::collapsed(focus)
        });
        Ok(())
    }
}

/// The line and position of an emitted run's character.
fn normalize(
    position: TextPosition,
    runs: &[(NodeId, usize, usize)],
) -> Result<(usize, TextPosition), TextError> {
    let index = runs
        .iter()
        .position(|(id, ..)| *id == position.node)
        .ok_or(TextError::InvalidRange)?;
    let (_, count, block) = runs[index];
    if position.character_index > count {
        return Err(TextError::InvalidRange);
    }
    // Parley 0.11.1 indexes past the current run at this boundary. Its export
    // orders runs/spans by source position, so the next span's start, on the
    // next line after a break, is equivalent.
    if position.character_index == count
        && let Some(&(node, _, next)) = runs.get(index + 1)
    {
        return Ok((
            next,
            TextPosition {
                node,
                character_index: 0,
            },
        ));
    }
    Ok((block, position))
}

fn color(value: Color) -> accesskit::Color {
    let [red, green, blue, alpha] = value.to_rgba();
    accesskit::Color {
        red,
        green,
        blue,
        alpha,
    }
}
