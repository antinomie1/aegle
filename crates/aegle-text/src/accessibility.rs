use accesskit::{Node, NodeId, Role, TextPosition, TextSelection, TreeUpdate};
use aegle_types::{Color, Point};
#[cfg(feature = "text-parley")]
use parley::Cursor;

use crate::{EditorDriver, TextError, document::Caret, nav::Selected};

/// One emitted text run: its semantic node, character count and the paragraph
/// and byte offset it starts at.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AccessRun {
    pub node: NodeId,
    pub chars: usize,
    pub block: usize,
    /// Byte offset of the run's first character within its paragraph.
    pub start: usize,
}

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
            #[cfg(feature = "text-parley")]
            if let crate::layout::Layout::Parley(ref p) = block.layout {
                let start = update.nodes.len();
                block.access.build_nodes(
                    &doc.text[block.shaped()],
                    &p.raw,
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
                let mut offset = 0;
                for (id, emitted) in &update.nodes[start..] {
                    let chars = emitted.character_lengths().len();
                    editor.access_runs.push(AccessRun {
                        node: *id,
                        chars,
                        block: index,
                        start: offset,
                    });
                    offset += chars;
                }
                continue;
            }
            // The system backend emits one run per paragraph, split at the
            // composition so its underline covers exactly the preedit.
            let shaped = block.shaped();
            let text = &doc.text[shaped.clone()];
            let mut bounds = vec![0usize];
            if let Some(compose) = &doc.compose {
                let start = compose.start.clamp(shaped.start, shaped.end) - shaped.start;
                let end = compose.end.clamp(shaped.start, shaped.end) - shaped.start;
                bounds.extend([start, end]);
            }
            bounds.push(text.len());
            bounds.sort_unstable();
            bounds.dedup();
            if text.is_empty() {
                bounds.push(0);
            }
            let mut emitted = false;
            for pair in bounds.windows(2) {
                let (from, to) = (pair[0], pair[1]);
                // An empty paragraph still needs one run for cursor positions.
                if from == to && emitted {
                    continue;
                }
                emitted = true;
                let run_text = &text[from..to];
                let chars = run_text.chars().count();
                let run_id = next_node_id();
                let mut run_node = Node::new(Role::TextRun);
                run_node.set_value(run_text.to_string());
                run_node.set_character_lengths(vec![1; chars].into_boxed_slice());
                run_node.set_foreground_color(foreground);
                let underlined = doc
                    .compose
                    .as_ref()
                    .is_some_and(|c| c.start <= shaped.start + from && shaped.start + to <= c.end);
                if underlined {
                    run_node.set_underline(accesskit::TextDecoration {
                        style: accesskit::TextDecorationStyle::Solid,
                        color: foreground,
                    });
                }
                node.push_child(run_id);
                update.nodes.push((run_id, run_node));
                editor.access_runs.push(AccessRun {
                    node: run_id,
                    chars,
                    block: index,
                    start: from,
                });
            }
        }
        let position = |caret: Caret| {
            #[cfg_attr(not(feature = "text-parley"), allow(unused_variables))]
            let (index, local, affinity) = editor.doc.local_caret(caret);
            let block = &editor.doc.blocks[index];
            #[cfg(feature = "text-parley")]
            if let crate::layout::Layout::Parley(ref p) = block.layout {
                let parley_aff = match affinity {
                    crate::layout::Affinity::Downstream => parley::Affinity::Downstream,
                    crate::layout::Affinity::Upstream => parley::Affinity::Upstream,
                };
                let cursor = Cursor::from_byte_index(&p.raw, local, parley_aff);
                return cursor.to_access_position(&p.raw, &block.access);
            }
            let local = local.min(block.range.len());
            let run = editor
                .access_runs
                .iter()
                .rfind(|run| run.block == index && run.start <= local)?;
            let text = &editor.doc.text[block.range.start + run.start..block.range.start + local];
            Some(TextPosition {
                node: run.node,
                character_index: text.chars().count(),
            })
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
            let (index, pos) = normalize(position, &self.editor.access_runs)?;
            let doc = &self.editor.doc;
            let block = &doc.blocks[index];
            #[cfg(feature = "text-parley")]
            if let crate::layout::Layout::Parley(ref p) = block.layout {
                let cursor = Cursor::from_access_position(&pos, &p.raw, &block.access)
                    .ok_or(TextError::InvalidRange)?;
                let aff = match cursor.affinity() {
                    parley::Affinity::Downstream => crate::layout::Affinity::Downstream,
                    parley::Affinity::Upstream => crate::layout::Affinity::Upstream,
                };
                return Ok(Caret {
                    index: block.range.start + cursor.index(),
                    affinity: aff,
                });
            }
            let run = doc
                .blocks
                .get(index)
                .and_then(|_| {
                    self.editor
                        .access_runs
                        .iter()
                        .find(|run| run.node == pos.node)
                })
                .ok_or(TextError::InvalidRange)?;
            let text = &doc.text[block.range.clone()];
            let byte_offset = text[run.start.min(text.len())..]
                .char_indices()
                .nth(pos.character_index)
                .map(|(i, _)| run.start.min(text.len()) + i)
                .unwrap_or(text.len());
            Ok(Caret {
                index: block.range.start + byte_offset,
                affinity: crate::layout::Affinity::Downstream,
            })
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
    runs: &[AccessRun],
) -> Result<(usize, TextPosition), TextError> {
    let index = runs
        .iter()
        .position(|run| run.node == position.node)
        .ok_or(TextError::InvalidRange)?;
    let run = runs[index];
    if position.character_index > run.chars {
        return Err(TextError::InvalidRange);
    }
    // Parley 0.11.1 indexes past the current run at this boundary. Its export
    // orders runs/spans by source position, so the next span's start, on the
    // next line after a break, is equivalent.
    if position.character_index == run.chars
        && let Some(next) = runs.get(index + 1)
    {
        return Ok((
            next.block,
            TextPosition {
                node: next.node,
                character_index: 0,
            },
        ));
    }
    Ok((run.block, position))
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
