//! Gradient backgrounds under the paint transition: stops and geometry move
//! from the shown gradient, or the background color, to the target. Samples
//! are written into the node's gradient in place, so a running transition
//! allocates nothing per frame.

use std::{collections::HashMap, f32::consts::PI, time::Duration};

use aegle_core::{Dirty, NodeId};
use aegle_motion::{Animation, Interpolate};
use aegle_scene::{Gradient, GradientGeometry, GradientStop};
use aegle_types::Point;

use crate::{
    Color, Result, TransitionProperty, motion::Running, state::Element, state::State,
    style::Decoration,
};

/// Where stops run: a linear gradient by its center, direction and half
/// length, which cannot collapse in between; a radial one as it is.
#[derive(Clone, Copy)]
enum Line {
    Linear {
        center: Point,
        angle: f32,
        half: f32,
    },
    Radial {
        center: Point,
        radius: f32,
    },
}

/// One end of a running transition, padded to the stop count of both.
#[derive(Clone, Copy)]
struct End {
    line: Line,
    stops: [GradientStop; Gradient::MAX_STOPS],
}

impl End {
    fn of(gradient: &Gradient) -> Self {
        let line = match gradient.geometry() {
            GradientGeometry::Linear { start, end } => {
                let (x, y) = ((end.x - start.x) / 2.0, (end.y - start.y) / 2.0);
                Line::Linear {
                    center: Point::new(start.x + x, start.y + y),
                    angle: y.atan2(x),
                    half: x.hypot(y),
                }
            }
            GradientGeometry::Radial { center, radius } => Line::Radial { center, radius },
        };
        let stops = gradient.stops();
        // Repeating the last stop leaves the drawing unchanged.
        let stops = std::array::from_fn(|i| stops[i.min(stops.len() - 1)]);
        Self { line, stops }
    }
    /// `gradient`'s geometry and offsets, all in `color`: a plain background.
    fn flat(gradient: &Gradient, color: Color) -> Self {
        let mut end = Self::of(gradient);
        end.stops.iter_mut().for_each(|stop| stop.color = color);
        end
    }
    fn geometry(&self) -> GradientGeometry {
        match self.line {
            Line::Linear {
                center,
                angle,
                half,
            } => linear(center, angle, half),
            Line::Radial { center, radius } => GradientGeometry::Radial { center, radius },
        }
    }
}

fn linear(center: Point, angle: f32, half: f32) -> GradientGeometry {
    let (x, y) = (angle.cos() * half, angle.sin() * half);
    GradientGeometry::Linear {
        start: Point::new(center.x - x, center.y - y),
        end: Point::new(center.x + x, center.y + y),
    }
}

/// The two ends of a transition.
struct Ends {
    from: End,
    to: End,
}

/// A running gradient transition.
pub struct Morph {
    ends: Ends,
    /// What was set: the end state, `None` returning to the background color.
    target: Option<Gradient>,
    /// The stop storage not given to the node. The two swap each frame,
    /// as the last frame's records still share the other.
    spare: Gradient,
    /// Progress from 0 to 1.
    pub curve: Running<f32>,
}

impl Ends {
    fn geometry(&self, progress: f32) -> GradientGeometry {
        let lerp = |a: f32, b: f32| a * (1.0 - progress) + b * progress;
        let point = |a: Point, b: Point| Point::new(lerp(a.x, b.x), lerp(a.y, b.y));
        match (self.from.line, self.to.line) {
            (
                Line::Linear {
                    center,
                    angle,
                    half,
                },
                Line::Linear {
                    center: c,
                    angle: a,
                    half: h,
                },
            ) => {
                // The shorter way round.
                let turn = (a - angle + PI).rem_euclid(2.0 * PI) - PI;
                linear(point(center, c), angle + turn * progress, lerp(half, h))
            }
            (
                Line::Radial { center, radius },
                Line::Radial {
                    center: c,
                    radius: r,
                },
            ) => GradientGeometry::Radial {
                center: point(center, c),
                radius: lerp(radius, r),
            },
            // Between kinds the geometry is the target's from the start.
            _ => self.to.geometry(),
        }
    }
    fn stop(&self, index: usize, progress: f32) -> GradientStop {
        let (a, b) = (self.from.stops[index], self.to.stops[index]);
        GradientStop {
            // Blending two nondecreasing lists keeps them nondecreasing.
            offset: (a.offset * (1.0 - progress) + b.offset * progress).clamp(0.0, 1.0),
            color: a.color.interpolate(b.color, progress),
        }
    }
    /// A gradient of `count` stops at `progress`, in new storage.
    fn build(&self, count: usize, progress: f32) -> Result<Gradient> {
        let stops: Vec<_> = (0..count).map(|i| self.stop(i, progress)).collect();
        Ok(match self.geometry(progress) {
            GradientGeometry::Linear { start, end } => Gradient::linear(start, end, &stops)?,
            GradientGeometry::Radial { center, radius } => {
                Gradient::radial(center, radius, &stops)?
            }
        })
    }
}

impl State {
    /// Starts, retargets or snaps the background gradient under the paint
    /// timing. A control not yet painted, or a change from no gradient to
    /// none, changes immediately.
    pub(crate) fn transition_gradient(&mut self, id: NodeId, target: Option<Gradient>) -> Result {
        let (policy, timing) = self.timing(id, TransitionProperty::Paint);
        let shown = self.decorations.get(&id).and_then(|d| d.gradient.clone());
        let running = self.motion.gradients.remove(&id);
        let goal = running.as_ref().map_or(&shown, |m| &m.target);
        if *goal == target {
            if let Some(running) = running {
                self.motion.gradients.insert(id, running);
            }
            return Ok(());
        }
        let painted = self.tree.get(id).unwrap().context.painted.is_some();
        let ends = match (&shown, &target) {
            (Some(shown), Some(target)) => Some((End::of(shown), End::of(target))),
            (None, Some(target)) => {
                let from = self.presented_appearance(id)?.background;
                Some((End::flat(target, from), End::of(target)))
            }
            (Some(shown), None) => {
                let to = self.appearance(id)?.background;
                Some((End::of(shown), End::flat(shown, to)))
            }
            (None, None) => None,
        };
        if let (Some(timing), true, Some((from, to))) = (timing, painted, ends) {
            let count = [&shown, &target]
                .into_iter()
                .flatten()
                .map(|g| g.stops().len())
                .max()
                .unwrap();
            let ends = Ends { from, to };
            let start = ends.build(count, 0.0)?;
            let morph = Morph {
                spare: ends.build(count, 0.0)?,
                ends,
                target,
                curve: Running::new(Animation::tween(0.0, 1.0, timing)?),
            };
            self.decorations.entry(id).or_default().gradient = Some(start);
            self.motion.gradients.insert(id, morph);
        } else {
            self.decorations.entry(id).or_default().gradient = target;
            self.trim_decoration(id);
            if running.is_some() && policy {
                self.complete(id);
            }
        }
        self.tree.mark_dirty(id, Dirty::PAINT)?;
        Ok(())
    }

    /// Jumps a running gradient transition to its target. Returns whether one ran.
    pub(crate) fn snap_gradient(&mut self, id: NodeId) -> bool {
        let Some(morph) = self.motion.gradients.remove(&id) else {
            return false;
        };
        self.decorations.entry(id).or_default().gradient = morph.target;
        self.trim_decoration(id);
        self.tree
            .mark_dirty(id, Dirty::PAINT)
            .expect("transitions belong to live nodes");
        true
    }

    /// The gradient a node rests at: a running transition's target.
    pub(crate) fn target_gradient(&self, id: NodeId) -> Option<Gradient> {
        match self.motion.gradients.get(&id) {
            Some(morph) => morph.target.clone(),
            None => self.decorations.get(&id).and_then(|d| d.gradient.clone()),
        }
    }
}

/// Samples running gradient transitions into the nodes' decorations.
pub(crate) fn advance(
    running: &mut HashMap<NodeId, Morph>,
    tree: &mut aegle_core::Tree<aegle_layout::LayoutNode<Element>>,
    decorations: &mut HashMap<NodeId, Decoration>,
    now: Duration,
    finished: &mut Vec<NodeId>,
) {
    running.retain(|id, morph| {
        let Some(elapsed) = morph.curve.elapsed(now) else {
            return true;
        };
        // Hidden subtrees go straight to their target.
        let visible = tree.get(*id).unwrap().context.effective_visible;
        let done = morph.curve.curve.finished(elapsed) || !visible;
        let decoration = decorations.entry(*id).or_default();
        if done {
            decoration.gradient = morph.target.take();
            if decoration.is_empty() {
                decorations.remove(id);
            }
            finished.push(*id);
        } else {
            let progress = morph.curve.curve.sample(elapsed).clamp(0.0, 1.0);
            let shown = decoration
                .gradient
                .as_mut()
                .expect("a running gradient is shown");
            std::mem::swap(shown, &mut morph.spare);
            let geometry = morph.ends.geometry(progress);
            shown
                .update(geometry, |i| morph.ends.stop(i, progress))
                .expect("blended gradients stay valid");
        }
        tree.mark_dirty(*id, Dirty::PAINT)
            .expect("transitions belong to live nodes");
        !done
    });
}
