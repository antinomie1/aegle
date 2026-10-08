//! Group effects: a subtree drawn as one layer at an opacity, optionally over
//! a blurred backdrop, and the damage they need.

use aegle_core::NodeId;
use aegle_scene::{Affine, Layer, RoundedRect, Scene};
use aegle_types::Rect;

use crate::{Node, Result, UiError, state::State};

/// A subtree's group opacity and backdrop blur.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Group {
    /// Opacity of the subtree drawn as one image, in `0..=1`.
    pub opacity: f32,
    /// Standard deviation in logical pixels of the blur behind the node.
    pub backdrop_blur: f32,
}

impl Group {
    /// No effect: fully opaque, nothing blurred.
    pub const NONE: Self = Self {
        opacity: 1.0,
        backdrop_blur: 0.0,
    };
    /// Whether drawing needs a layer.
    pub fn layered(self) -> bool {
        self.opacity < 1.0 || self.backdrop_blur > 0.0
    }
}

/// What a node's layer covered and looked like when last refreshed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Drawn {
    group: Option<Group>,
    area: Option<Rect>,
}

/// One step of drawing a UI, from [`crate::Ui::visit_scenes`], in order.
#[derive(Clone, Copy, Debug)]
pub enum Visit<'a> {
    /// Draw a retained record placed by `transform` in logical window
    /// coordinates and cut to the optional ancestor clip, which is in window
    /// coordinates outside the transform.
    Scene {
        /// The node's record.
        scene: &'a Scene,
        /// Local to window coordinates.
        transform: Affine,
        /// Ancestor scroll clip.
        clip: Option<Rect>,
    },
    /// Draw everything until the matching [`Visit::PopLayer`] as one layer,
    /// in logical window coordinates; hosts map it like the scene transforms
    /// (for example `layer.then(scale)`) and hand it to their renderer.
    PushLayer(Layer),
    /// Composite the innermost layer.
    PopLayer,
}

impl Node {
    /// Draws this subtree as one image at `opacity` (`0..=1`): overlapping
    /// descendants do not show through each other, unlike translucent colors.
    /// Hit testing, focus and accessibility are unchanged; zero draws
    /// nothing. With `motion`, a control with an opacity timing animates.
    /// Renderers draw it through an offscreen layer.
    pub fn set_opacity(&self, opacity: f32) -> Result {
        if !(0.0..=1.0).contains(&opacity) {
            return Err(UiError::InvalidValue.into());
        }
        self.change(|state, id| {
            state.groups.entry(id).or_default();
            #[cfg(feature = "motion")]
            return state.transition_opacity(id, opacity);
            #[cfg(not(feature = "motion"))]
            {
                state.tree.get_mut(id).unwrap().context.group.opacity = opacity;
                state.repaint = true;
                Ok(())
            }
        })
    }
    /// The logical target opacity.
    pub fn opacity(&self) -> Result<f32> {
        self.change(|state, id| Ok(state.target_opacity(id)))
    }
    /// Blurs what lies behind this node within its rounded bounds before
    /// drawing it, with a Gaussian of standard deviation `blur` logical
    /// pixels; zero removes it. The blurred backdrop is drawn at the node's
    /// opacity. Only what this window draws is blurred, not the desktop
    /// behind a transparent window.
    pub fn set_backdrop_blur(&self, blur: f32) -> Result {
        if !(blur.is_finite() && blur >= 0.0) {
            return Err(UiError::InvalidValue.into());
        }
        self.change(|state, id| {
            state.groups.entry(id).or_default();
            state.tree.get_mut(id).unwrap().context.group.backdrop_blur = blur;
            state.repaint = true;
            Ok(())
        })
    }
    /// The backdrop blur set by [`Self::set_backdrop_blur`].
    pub fn backdrop_blur(&self) -> Result<f32> {
        self.change(|state, id| Ok(state.tree.get(id).unwrap().context.group.backdrop_blur))
    }
}

impl State {
    /// The target opacity, past any running fade.
    pub fn target_opacity(&self, id: NodeId) -> f32 {
        #[cfg(feature = "motion")]
        if let Some(running) = self.motion.fading.get(&id) {
            return running.target();
        }
        self.tree.get(id).unwrap().context.group.opacity
    }

    /// Starts, retargets or snaps the opacity, like `transition_offset`.
    #[cfg(feature = "motion")]
    pub fn transition_opacity(&mut self, id: NodeId, target: f32) -> Result {
        use crate::{TransitionProperty, motion::Step};
        let (policy, timing) = self.timing(id, TransitionProperty::Opacity);
        let current = self.tree.get(id).unwrap().context.group.opacity;
        let fading = &mut self.motion.fading;
        match crate::motion::plan(fading, id, current, target, timing)? {
            Step::Unchanged => {}
            Step::Started => self.repaint = true,
            Step::Snapped => {
                self.tree.get_mut(id).unwrap().context.group.opacity = target;
                self.repaint = true;
                if policy {
                    self.complete(id);
                }
            }
        }
        Ok(())
    }

    /// Jumps a running fade to its target. Returns whether one was running.
    pub fn snap_opacity(&mut self, id: NodeId) -> bool {
        #[cfg(feature = "motion")]
        if let Some(running) = self.motion.fading.remove(&id) {
            self.tree.get_mut(id).unwrap().context.group.opacity = running.target();
            self.repaint = true;
            return true;
        }
        let _ = id;
        false
    }

    /// The layer a node's subtree draws through, if its group needs one and
    /// anything of it can show.
    pub(crate) fn layer(&self, id: NodeId) -> Result<Option<Layer>> {
        let element = &self.tree.get(id).unwrap().context;
        let group = element.group;
        if !group.layered() || !element.effective_visible {
            return Ok(None);
        }
        let shape = self.shape_area(id);
        let extent = match (self.subtree_area(id), shape) {
            (Some(content), Some(shape)) if group.backdrop_blur > 0.0 => content.union(shape),
            (Some(content), _) => content,
            (None, Some(shape)) if group.backdrop_blur > 0.0 => shape,
            _ => return Ok(None),
        };
        let size = element.bounds.size;
        let radius = self.presented_radius(id)?;
        let shape = RoundedRect::new(Rect::new(0.0, 0.0, size.width, size.height), radius)?;
        let place = Affine::translation(element.bounds.origin.x, element.bounds.origin.y)?;
        let transform = element.xf.map_or(Ok(place), |xf| place.then(xf))?;
        let layer = Layer::new(
            shape,
            transform,
            extent,
            element.clip,
            group.opacity,
            group.backdrop_blur,
        )?;
        Ok(Some(layer))
    }

    #[cfg(feature = "motion")]
    fn presented_radius(&self, id: NodeId) -> Result<f32> {
        Ok(self.presented_appearance(id)?.radius)
    }
    #[cfg(not(feature = "motion"))]
    fn presented_radius(&self, id: NodeId) -> Result<f32> {
        Ok(self.appearance(id)?.radius)
    }

    /// The presented, clipped window area of a node's bounds.
    fn shape_area(&self, id: NodeId) -> Option<Rect> {
        let element = &self.tree.get(id).unwrap().context;
        let shown = element.xf.map_or(element.bounds, |xf| {
            crate::scroll::map_rect(xf, element.bounds)
        });
        match element.clip {
            Some(clip) => clip.intersection(shown),
            None => Some(shown),
        }
    }

    /// Everything a subtree's records covered at the last refresh.
    fn subtree_area(&self, id: NodeId) -> Option<Rect> {
        let element = &self.tree.get(id).unwrap().context;
        if !element.effective_visible {
            return None;
        }
        let mut area = element.painted;
        for child in self.tree.children(id).unwrap() {
            area = match (area, self.subtree_area(child)) {
                (Some(a), Some(b)) => Some(a.union(b)),
                (a, b) => a.or(b),
            };
        }
        area
    }

    /// After records are damaged: damages groups whose opacity, blur or area
    /// changed, then the whole sampled area of every backdrop blur that any
    /// damage reaches, since its result depends on all of it.
    pub(crate) fn damage_groups(&mut self) {
        if self.groups.is_empty() {
            return;
        }
        let ids: Vec<_> = self.groups.keys().copied().collect();
        let mut blurs = Vec::new();
        for id in ids {
            let element = &self.tree.get(id).unwrap().context;
            let group = element.group;
            let now = Drawn {
                group: (group.layered() && element.effective_visible).then_some(group),
                area: self.subtree_area(id),
            };
            let drawn = self.groups[&id];
            if drawn != now && (drawn.group.is_some() || now.group.is_some()) {
                for area in [drawn.area, now.area].into_iter().flatten() {
                    self.damage.add(area);
                }
                self.repaint = true;
            }
            if now.group.is_none() && !self.motion_fading(id) && group == Group::NONE {
                self.groups.remove(&id);
                continue;
            }
            *self.groups.get_mut(&id).unwrap() = now;
            if let (Some(group), Some(shape)) = (now.group, self.shape_area(id))
                && group.backdrop_blur > 0.0
            {
                let reach = 3.0 * group.backdrop_blur + 2.0;
                blurs.push(Rect::new(
                    shape.origin.x - reach,
                    shape.origin.y - reach,
                    shape.size.width + 2.0 * reach,
                    shape.size.height + 2.0 * reach,
                ));
            }
        }
        if self.damage_full {
            return;
        }
        // Each pass adds at least one sampled area, so this ends.
        loop {
            let reached = blurs.iter().position(|&sampled| {
                let rects = self.damage.rects();
                rects.iter().any(|r| r.intersection(sampled).is_some())
                    && !rects.iter().any(|r| contains(*r, sampled))
            });
            let Some(index) = reached else {
                break;
            };
            self.damage.add(blurs.swap_remove(index));
            self.repaint = true;
        }
    }

    fn motion_fading(&self, id: NodeId) -> bool {
        #[cfg(feature = "motion")]
        return self.motion.fading.contains_key(&id);
        #[cfg(not(feature = "motion"))]
        {
            let _ = id;
            false
        }
    }
}

fn contains(outer: Rect, inner: Rect) -> bool {
    outer.origin.x <= inner.origin.x
        && outer.origin.y <= inner.origin.y
        && outer.origin.x + outer.size.width >= inner.origin.x + inner.size.width
        && outer.origin.y + outer.size.height >= inner.origin.y + inner.size.height
}
