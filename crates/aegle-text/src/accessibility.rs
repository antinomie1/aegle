use accesskit::{Node, NodeId, Role, TextPosition, TextSelection, TreeUpdate};
use aegle_types::{Color, Point};

use crate::{EditorDriver, TextError};

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
    /// The committed value remains [`crate::Editor::text`]. Protected/password
    /// text is not supported by this plain-text bridge.
    ///
    /// Parley retains run identities; Aegle additionally retains only each ID
    /// and its character count for checked actions. Emitting nodes allocates
    /// their text/geometry properties, so call only for requested semantic
    /// updates. The platform adapter owns the emitted snapshot.
    pub fn accessibility(
        &mut self,
        update: &mut TreeUpdate,
        node: &mut Node,
        next_node_id: impl FnMut() -> NodeId,
        origin: Point,
    ) -> Result<(), TextError> {
        self.ready()?;
        if !origin.x.is_finite() || !origin.y.is_finite() {
            return Err(TextError::InvalidPosition);
        }
        node.set_role(if self.editor.multiline {
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
        let start = update.nodes.len();
        self.engine().accessibility(
            update,
            node,
            next_node_id,
            origin.x.into(),
            origin.y.into(),
            |node, style| {
                node.set_foreground_color(color(style.brush));
                if let Some(underline) = &style.underline {
                    node.set_underline(accesskit::TextDecoration {
                        style: accesskit::TextDecorationStyle::Solid,
                        color: color(underline.brush),
                    });
                }
            },
        );
        self.editor.access_runs.clear();
        self.editor.access_runs.extend(
            update.nodes[start..]
                .iter()
                .map(|(id, node)| (*id, node.character_lengths().len())),
        );
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
        let selection = TextSelection {
            anchor: normalize(selection.anchor, &self.editor.access_runs)?,
            focus: normalize(selection.focus, &self.editor.access_runs)?,
        };
        let before = self.editor.inner.generation();
        self.engine().select_from_accesskit(&selection);
        self.moved(before);
        Ok(())
    }
}

fn normalize(position: TextPosition, runs: &[(NodeId, usize)]) -> Result<TextPosition, TextError> {
    let index = runs
        .iter()
        .position(|(id, _)| *id == position.node)
        .ok_or(TextError::InvalidRange)?;
    let count = runs[index].1;
    if position.character_index > count {
        return Err(TextError::InvalidRange);
    }
    // Parley 0.11.1 indexes past the current run at this boundary. Its export
    // orders runs/spans by source position, so the next span's start is equivalent.
    if position.character_index == count {
        if let Some((node, _)) = runs.get(index + 1) {
            return Ok(TextPosition {
                node: *node,
                character_index: 0,
            });
        }
    }
    Ok(position)
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
