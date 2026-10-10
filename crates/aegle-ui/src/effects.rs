//! Node shadows and gradient backgrounds, drawn with the scene's native
//! effect commands on every backend.

use crate::{Color, Node, OrFail, Point, Result, Shadow, UiError, style::Decoration};
use crate::{ColorSlot, TokenSlot};
use aegle_core::{Dirty, NodeId};
use aegle_scene::{Gradient, GradientGeometry, Rect, RoundedRect, SceneBuilder};
use aegle_types::Size;

/// This shadow fully transparent, what it fades from or to.
#[cfg(feature = "motion")]
fn clear(shadow: Shadow) -> Shadow {
    let [r, g, b, _] = shadow.color.to_rgba();
    Shadow {
        color: Color::rgba(r, g, b, 0),
        ..shadow
    }
}

impl Node {
    /// Draws `shadow` beneath this node, following its corner radius, or
    /// removes it with `None`; a fully transparent shadow is `None`. It
    /// extends beyond the node's bounds without affecting layout or hit
    /// testing; ancestors' clips still apply. With a
    /// `TransitionProperty::Shadow` timing (the `motion` feature), offset, blur, spread and color
    /// tween from the shown shadow, and a shadow appears or goes by fading
    /// its color. Ends a [`Self::bind_shadow`] binding.
    pub fn set_shadow(&self, shadow: impl Into<Option<Shadow>>) {
        let shadow = shadow.into();
        if shadow.is_some_and(|s| !s.is_valid()) {
            panic!("{}", UiError::InvalidValue);
        }
        self.change(|state, id| {
            state.write_unbound(id, TokenSlot::Shadow, |state| {
                state.transition_shadow(id, shadow)
            })
        })
    }

    /// The shadow set by [`Self::set_shadow`], past any running transition.
    pub fn shadow(&self) -> Option<Shadow> {
        self.change(|state, id| {
            #[cfg(feature = "motion")]
            if let Some(running) = state.motion.shadows.get(&id) {
                return Ok(Some(running.target()).filter(|s| !s.is_clear()));
            }
            Ok(state.decorations.get(&id).and_then(|d| d.shadow))
        })
    }

    /// Fills this node's background with `gradient` instead of its background
    /// color, or restores the color with `None`. The gradient's coordinates are
    /// fractions of the node's size: x of its width and y of its height, with a
    /// radial radius in fractions of its larger side. Controls that paint no
    /// background ignore it. Ends the bindings of its stop colors.
    pub fn set_background_gradient(&self, gradient: impl Into<Option<Gradient>>) {
        let gradient = gradient.into();
        self.change(|state, id| {
            state.set_gradient(id, gradient);
            state.tokens.unbind(id, |slot| {
                matches!(slot, TokenSlot::Color(ColorSlot::GradientStop(_)))
            });
            Ok(())
        })
    }

    /// The gradient set by [`Self::set_background_gradient`].
    pub fn background_gradient(&self) -> Option<Gradient> {
        self.change(|state, id| Ok(state.decorations.get(&id).and_then(|d| d.gradient.clone())))
    }
}

impl crate::State {
    fn set_gradient(&mut self, id: NodeId, gradient: Option<Gradient>) {
        self.decorations.entry(id).or_default().gradient = gradient;
        self.trim_decoration(id);
        self.tree
            .mark_dirty(id, Dirty::PAINT)
            .expect("callers hold a live node");
    }

    /// Recolors stop `index` of the node's gradient, as a token binding does.
    pub(crate) fn set_gradient_stop(&mut self, id: NodeId, index: u8, color: Color) -> Result {
        let gradient = self.decorations.get(&id).and_then(|d| d.gradient.as_ref());
        let gradient = gradient.ok_or(UiError::InvalidValue)?;
        let mut stops = gradient.stops().to_vec();
        let stop = stops
            .get_mut(usize::from(index))
            .ok_or(UiError::InvalidValue)?;
        if stop.color == color {
            return Ok(());
        }
        stop.color = color;
        let gradient = match gradient.geometry() {
            GradientGeometry::Linear { start, end } => Gradient::linear(start, end, &stops),
            GradientGeometry::Radial { center, radius } => Gradient::radial(center, radius, &stops),
        };
        self.set_gradient(id, Some(gradient?));
        Ok(())
    }

    /// Starts, retargets or snaps a shadow, like `transition_offset`; a fully
    /// transparent target is none.
    pub fn transition_shadow(&mut self, id: NodeId, target: Option<Shadow>) -> Result {
        let target = target.filter(|s| !s.is_clear());
        let shown = self.decorations.get(&id).and_then(|d| d.shadow);
        #[cfg(feature = "motion")]
        {
            use crate::{TransitionProperty, motion::Step};
            let (policy, timing) = self.timing(id, TransitionProperty::Shadow);
            let (Some(from), Some(to)) = (shown.or(target.map(clear)), target.or(shown.map(clear)))
            else {
                return Ok(());
            };
            match crate::motion::plan(&mut self.motion.shadows, id, from, to, timing)? {
                Step::Unchanged => return Ok(()),
                Step::Started => {
                    self.decorations.entry(id).or_default().shadow = Some(from);
                    self.tree.mark_dirty(id, Dirty::PAINT)?;
                    return Ok(());
                }
                Step::Snapped if policy => self.complete(id),
                Step::Snapped => {}
            }
        }
        if shown != target {
            self.decorations.entry(id).or_default().shadow = target;
            self.trim_decoration(id);
            self.tree.mark_dirty(id, Dirty::PAINT)?;
        }
        Ok(())
    }

    /// Jumps a running shadow transition to its target. Returns whether one ran.
    pub fn snap_shadow(&mut self, id: NodeId) -> bool {
        #[cfg(feature = "motion")]
        if let Some(running) = self.motion.shadows.remove(&id) {
            let target = Some(running.target()).filter(|s| !s.is_clear());
            self.decorations.entry(id).or_default().shadow = target;
            self.trim_decoration(id);
            self.tree
                .mark_dirty(id, Dirty::PAINT)
                .expect("transitions belong to live nodes");
            return true;
        }
        let _ = id;
        false
    }
}

/// Samples running shadow transitions into the nodes' decorations; a shadow
/// that faded out is removed.
#[cfg(feature = "motion")]
pub(crate) fn advance_shadows(
    running: &mut std::collections::HashMap<aegle_core::NodeId, crate::motion::Running<Shadow>>,
    tree: &mut aegle_core::Tree<aegle_layout::LayoutNode<crate::state::Element>>,
    decorations: &mut std::collections::HashMap<aegle_core::NodeId, Decoration>,
    now: std::time::Duration,
    finished: &mut Vec<aegle_core::NodeId>,
) {
    running.retain(|id, run| {
        let Some(elapsed) = run.elapsed(now) else {
            return true;
        };
        // Hidden subtrees go straight to their target.
        let visible = tree.get(*id).unwrap().context.effective_visible;
        let done = run.curve.finished(elapsed) || !visible;
        let value = match done {
            true => Some(run.curve.target()).filter(|s| !s.is_clear()),
            false => Some(run.curve.sample(elapsed)),
        };
        let decoration = decorations.entry(*id).or_default();
        decoration.shadow = value;
        if decoration.is_empty() {
            decorations.remove(id);
        }
        tree.mark_dirty(*id, Dirty::PAINT)
            .expect("transitions belong to live nodes");
        if done {
            finished.push(*id);
        }
        !done
    });
}

/// Records a node's shadow, before its background.
pub(crate) fn paint_shadow(
    decoration: Option<&Decoration>,
    builder: &mut SceneBuilder,
    size: Size,
    radius: f32,
) {
    let Some(shadow) = decoration.and_then(|d| d.shadow) else {
        return;
    };
    let (width, height) = (
        (size.width + shadow.spread * 2.0).max(0.0),
        (size.height + shadow.spread * 2.0).max(0.0),
    );
    let rect = Rect::new(
        shadow.offset.x - shadow.spread,
        shadow.offset.y - shadow.spread,
        width,
        height,
    );
    let shape = RoundedRect::new(rect, (radius + shadow.spread).max(0.0));
    builder.shadow(shape, shadow.color, shadow.blur);
}

/// Records a node's gradient background in place of its color; returns
/// whether it had one.
pub(crate) fn paint_gradient(
    decoration: Option<&Decoration>,
    builder: &mut SceneBuilder,
    shape: RoundedRect,
) -> bool {
    let Some(gradient) = decoration.and_then(|d| d.gradient.as_ref()) else {
        return false;
    };
    let size = shape.rect().size;
    let at = |p: Point| Point::new(p.x * size.width, p.y * size.height);
    let geometry = match gradient.geometry() {
        GradientGeometry::Linear { start, end } => GradientGeometry::Linear {
            start: at(start),
            end: at(end),
        },
        GradientGeometry::Radial { center, radius } => GradientGeometry::Radial {
            center: at(center),
            radius: radius * size.width.max(size.height),
        },
    };
    // A collapsed node draws nothing, and has no extent for the gradient to span.
    if !shape.is_empty() {
        builder.fill_gradient(shape, &gradient.with_geometry(geometry).or_fail());
    }
    true
}
