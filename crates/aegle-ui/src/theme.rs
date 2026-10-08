use crate::{Node, Result, Theme, ThemeOverride, Ui, UiError, state::State};
use aegle_core::{Dirty, NodeId};
use std::rc::Rc;

impl Ui {
    /// Replaces the theme while preserving editor text, focus, selection and preedit.
    /// Palette-only changes do not reshape text or recompute layout. Local layout
    /// setters and subtrees with a local theme remain authoritative. If a bound
    /// property rejects its new value, the old theme is restored and the error returned.
    pub fn set_theme(&self, theme: Theme) -> Result {
        theme.validate()?;
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        let old = state.theme;
        if old == theme {
            return Ok(());
        }
        let outcome = state.replace_theme(&old, &theme);
        if outcome.is_err() {
            state.replace_theme(&theme, &old)?;
        }
        outcome
    }
}

impl Node {
    /// Gives this control and its descendants their own theme snapshot, or with
    /// `None` returns them to the parent's. A nested local theme wins over an
    /// ancestor's; local layout, font size and visual overrides still win over both.
    /// Created and reparented controls inherit their new parent's resolved theme.
    /// Fails without a change if a bound property rejects its new value.
    pub fn set_theme(&self, theme: Option<Theme>) -> Result {
        if let Some(theme) = &theme {
            theme.validate()?;
        }
        self.change(|state, id| state.set_local_theme(id, None, theme.map(Rc::new)))
    }
    /// Gives this subtree the parent's theme with the set tokens replaced. Unlike a
    /// snapshot from [`Self::set_theme`], it follows later changes to the parent
    /// or UI theme. `None` removes the override. Nested overrides and local
    /// themes work as for `set_theme`; font size and layout overrides still win.
    pub fn set_theme_override(&self, theme: Option<ThemeOverride>) -> Result {
        self.change(|state, id| {
            if let Some(theme) = theme {
                let parent = state
                    .tree
                    .parent(id)?
                    .map_or(state.theme, |p| *state.theme_of(p));
                theme.apply(&parent).validate()?;
            }
            state.set_local_theme(id, theme, None)
        })
    }
    /// The resolved theme: this control's local theme, the nearest ancestor's,
    /// or the UI theme.
    pub fn theme(&self) -> Result<Theme> {
        self.change(|state, id| Ok(*state.theme_of(id)))
    }
}

impl State {
    /// Moves the UI theme from `old` to `theme`, re-resolving the nodes that
    /// inherit it, theme overrides and bindings.
    fn replace_theme(&mut self, old: &Theme, theme: &Theme) -> Result {
        self.rebuild_order();
        for index in 0..self.order.len() {
            let id = self.order[index];
            if self.tree.get(id).unwrap().context.theme.is_none() {
                self.retheme(id, old, theme)?;
            }
        }
        self.theme = *theme;
        // Overrides re-resolve against their parent's new theme, outermost first.
        for index in 0..self.order.len() {
            let id = self.order[index];
            if self.overrides.contains_key(&id) {
                self.propagate_theme(id, None)?;
            }
        }
        self.refresh_tokens(None)
    }

    /// Gives a subtree a theme override, a local theme snapshot or neither,
    /// and re-resolves it; if a bound property rejects its new value, the
    /// previous ones are restored and the error returned.
    fn set_local_theme(
        &mut self,
        id: NodeId,
        tokens: Option<ThemeOverride>,
        local: Option<Rc<Theme>>,
    ) -> Result {
        let old_tokens = self.overrides.get(&id).copied();
        let element = &self.tree.get(id).unwrap().context;
        let old_local = (element.local_theme && old_tokens.is_none())
            .then(|| element.theme.clone())
            .flatten();
        let scope = |state: &mut Self, tokens: Option<ThemeOverride>, local| {
            match tokens {
                Some(tokens) => state.overrides.insert(id, tokens),
                None => state.overrides.remove(&id),
            };
            state.propagate_theme(id, local)
        };
        let outcome = scope(self, tokens, local);
        if outcome.is_err() {
            scope(self, old_tokens, old_local)?;
        }
        outcome
    }

    /// Re-resolves themes in the subtree of `id` from its parent and `local`.
    /// Nested local themes keep their snapshots.
    pub fn propagate_theme(&mut self, id: NodeId, local: Option<Rc<Theme>>) -> Result {
        self.rebuild_order();
        let start = self.order.iter().position(|&n| n == id).unwrap();
        for index in start..self.order.len() {
            let n = self.order[index];
            if !self.contains(id, n) {
                break;
            }
            let element = &self.tree.get(n).unwrap().context;
            let inherited = self
                .tree
                .parent(n)?
                .and_then(|p| self.tree.get(p).unwrap().context.theme.clone());
            let theme = if let Some(tokens) = self.overrides.get(&n) {
                let base = inherited.as_deref().unwrap_or(&self.theme);
                Some(Rc::new(tokens.apply(base)))
            } else if n == id && local.is_some() {
                local.clone()
            } else if n != id && element.local_theme {
                continue;
            } else {
                inherited
            };
            let old = *element.theme_or(&self.theme);
            let new = *theme.as_deref().unwrap_or(&self.theme);
            let element = &mut self.tree.get_mut(n).unwrap().context;
            element.theme = theme;
            if n == id {
                element.local_theme = local.is_some() || self.overrides.contains_key(&n);
            }
            if old != new {
                self.retheme(n, &old, &new)?;
            }
        }
        // Reparenting changes inherited custom tokens even with the same theme.
        self.refresh_tokens(Some(id))
    }

    /// Applies a node's resolved theme change to its text, default layout and
    /// paint, without recreating its editor.
    fn retheme(&mut self, id: NodeId, old: &Theme, theme: &Theme) -> Result {
        let is_root = id == self.root;
        // The window clear color is the root's background: areas no record
        // covers change with it.
        if is_root && theme.background != old.background {
            self.damage_full = true;
        }
        let font_changed = theme.font_size != old.font_size
            && self
                .decorations
                .get(&id)
                .and_then(|d| d.font_size)
                .is_none();
        let text_style = self.text_style_in(id, theme);
        let node = self.tree.get_mut(id).unwrap();
        let mut style = node.style().clone();
        if font_changed {
            if let Some(text) = node.context.control.paragraph_mut() {
                self.fonts.borrow_mut().restyle(text, &text_style)?;
            } else if let Some(field) = node.context.control.editor_mut() {
                self.fonts
                    .borrow_mut()
                    .edit(field.editor_mut())
                    .restyle(&text_style)?;
            }
        }
        node.context
            .control
            .retheme(theme, node.context.local_layout, is_root, &mut style);
        if style != *node.style() {
            aegle_layout::set_style(&mut self.tree, id, style)?;
        }
        let custom_skin = self.tree.get(id).unwrap().context.skin.is_some();
        let toggle_gap_changed =
            theme.gap != old.gap && self.tree.get(id).unwrap().context.control.uses_gap();
        let dirty = if font_changed || theme.padding != old.padding || toggle_gap_changed {
            Dirty::ALL
        } else if theme.foreground != old.foreground || theme.muted != old.muted || custom_skin {
            Dirty::PAINT | Dirty::SEMANTICS
        } else {
            Dirty::PAINT
        };
        self.tree.mark_dirty(id, dirty)?;
        self.repaint = true;
        self.ime_dirty = true;
        Ok(())
    }
}
