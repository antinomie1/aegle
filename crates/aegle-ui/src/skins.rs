//! Skins in scope: a skin for one node, or for every control of a kind in a
//! subtree. Each node keeps the skin that resolves for it, so painting never
//! searches ancestors.

use aegle_core::{Dirty, NodeId};

use crate::{ControlKind, Node, Result, Skin, state::State};

/// Where a skin applies: the node it is set on, or controls of a kind in
/// that node's subtree, the node included.
type Rule = (Option<&'static ControlKind>, Skin);

impl Node {
    /// Gives this control its own skin, or with `None` returns it to the one
    /// its kind resolves to in this subtree. Its result in the current state is
    /// validated first; later states are validated during refresh. Local
    /// style keeps precedence over every skin.
    pub fn set_skin(&self, skin: Option<Skin>) -> Result {
        self.change(|state, id| state.set_skin_rule(id, None, skin))
    }
    /// Skins every control of `kind` in this subtree, this one included, like
    /// a local theme: a nearer subtree's skin for the kind wins, and a
    /// control's own [`Self::set_skin`] wins over both. `None` removes it.
    /// Controls created or moved into the subtree follow it.
    pub fn set_kind_skin(&self, kind: &'static ControlKind, skin: Option<Skin>) -> Result {
        self.change(|state, id| state.set_skin_rule(id, Some(kind), skin))
    }
}

impl State {
    fn set_skin_rule(
        &mut self,
        id: NodeId,
        kind: Option<&'static ControlKind>,
        skin: Option<Skin>,
    ) -> Result {
        if let Some(skin) = skin
            && kind.is_none_or(|kind| self.tree.get(id).unwrap().context.control.kind() == kind)
        {
            skin(self.theme_of(id), self.visual_state(id)).validate()?;
        }
        let rules = self.skins.entry(id).or_default();
        rules.retain(|rule| rule.0 != kind);
        if let Some(skin) = skin {
            rules.push((kind, skin));
        }
        if rules.is_empty() {
            self.skins.remove(&id);
        }
        match kind {
            None => self.resolve_skin(id),
            Some(_) => self.resolve_skins(id),
        }
    }

    /// Re-resolves the skin of every node in the subtree of `id`, after a
    /// skin rule changed there or the subtree moved.
    pub(crate) fn resolve_skins(&mut self, id: NodeId) -> Result {
        self.rebuild_order();
        let start = self.order.iter().position(|&n| n == id).unwrap();
        for index in start..self.order.len() {
            let node = self.order[index];
            if node != id && !self.contains(id, node) {
                break;
            }
            self.resolve_skin(node)?;
        }
        Ok(())
    }

    /// Stores the skin that applies to `id`, repainting it if that changed.
    pub(crate) fn resolve_skin(&mut self, id: NodeId) -> Result {
        let skin = self.find_skin(id);
        let element = &mut self.tree.get_mut(id).unwrap().context;
        if element.skin.map(|s| s as usize) != skin.map(|s| s as usize) {
            element.skin = skin;
            self.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
        }
        Ok(())
    }

    /// The node's own skin, else the nearest skin for its kind; `None` uses
    /// the kind's default.
    fn find_skin(&self, id: NodeId) -> Option<Skin> {
        if self.skins.is_empty() {
            return None;
        }
        let find = |node, kind| {
            self.skins
                .get(&node)
                .and_then(|rules: &Vec<Rule>| rules.iter().find(|rule| rule.0 == kind))
                .map(|rule| rule.1)
        };
        let kind = self.tree.get(id).unwrap().context.control.kind();
        let mut node = Some(id);
        find(id, None).or_else(|| {
            while let Some(current) = node {
                if let Some(skin) = find(current, Some(kind)) {
                    return Some(skin);
                }
                node = self.tree.parent(current).ok().flatten();
            }
            None
        })
    }
}
