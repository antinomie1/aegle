// The engine state's fields and methods are the authoring surface for control
// libraries; the contract is described in `control` and on `State`.
#![allow(missing_docs)]

//! Painting a node's scene from its control, and the border helper controls share.

use crate::{Result, control::PaintCx, effects, state::State};
use aegle_core::NodeId;
use aegle_scene::{Color, Rect, RoundedRect, SceneBuilder};
use aegle_types::Size;

impl State {
    /// Re-records a node's scene (and a viewport's overlay) from its control.
    pub fn record(&mut self, id: NodeId) -> Result {
        let visual = self.visual_state(id);
        let appearance = self.appearance_for(id, visual)?;
        #[cfg(feature = "motion")]
        let appearance = self.transition_appearance(id, appearance)?;
        let bars = self.scrollbars(id);
        let bar_color = self.scrollbar_color(id);
        let theme = *self.theme_of(id);
        let time = self.frame_time;
        #[cfg(feature = "motion")]
        let reduced_motion = self.motion.reduced;
        #[cfg(not(feature = "motion"))]
        let reduced_motion = false;
        let mut next_frame = false;
        let decoration = self.decorations.get(&id);
        let element = &mut self.tree.get_mut(id).unwrap().context;
        let padding = element.inset(&theme);
        let size = element.bounds.size;
        let mut builder = std::mem::take(&mut element.scene).into_builder();
        builder.clear();
        let frame = element.control.frame();
        if element.effective_visible {
            let shape = RoundedRect::new(
                Rect::new(0.0, 0.0, size.width, size.height),
                appearance.radius,
            )?;
            effects::paint_shadow(decoration, &mut builder, size, appearance.radius)?;
            let gradient =
                frame.background && effects::paint_gradient(decoration, &mut builder, shape)?;
            if frame.background && !gradient && appearance.background.to_rgba()[3] != 0 {
                builder.fill(shape, appearance.background)?;
            }
            if frame.border {
                outline(
                    &mut builder,
                    size,
                    appearance.radius,
                    appearance.border_width,
                    appearance.border_color,
                )?;
            }
            let mut cx = PaintCx {
                builder: &mut builder,
                size,
                padding,
                theme: &theme,
                appearance: &appearance,
                visual,
                scroll: element.scroll,
                shape,
                bars,
                bar_color,
                time,
                reduced_motion,
                next_frame: false,
            };
            element.control.paint(&mut cx)?;
            next_frame = cx.next_frame;
            outline(
                &mut builder,
                size,
                appearance.radius,
                appearance.focus_width,
                appearance.focus_color,
            )?;
        }
        element.scene = builder.finish()?;
        if let Some(overlay) = &mut element.overlay {
            let mut builder = std::mem::take(&mut **overlay).into_builder();
            builder.clear();
            if element.effective_visible {
                element.control.paint_overlay(&mut PaintCx {
                    builder: &mut builder,
                    size,
                    padding,
                    theme: &theme,
                    appearance: &appearance,
                    visual,
                    scroll: element.scroll,
                    shape: RoundedRect::new(
                        Rect::new(0.0, 0.0, size.width, size.height),
                        appearance.radius,
                    )?,
                    bars,
                    bar_color,
                    time,
                    reduced_motion,
                    next_frame: false,
                })?;
            }
            **overlay = builder.finish()?;
        }
        if next_frame {
            self.animated.insert(id);
        } else {
            self.animated.remove(&id);
        }
        Ok(())
    }
}

// Keep centered scene strokes inside the logical bounds. Large requested widths
// saturate at half the smaller extent, preserving a drawable center rectangle.
/// Strokes a centered border inside `size`, saturating wide widths at half the
/// smaller extent; nothing is drawn for zero width or a transparent color.
pub fn outline(
    builder: &mut SceneBuilder,
    size: Size,
    radius: f32,
    width: f32,
    color: Color,
) -> Result {
    let width = width.min(size.width.min(size.height) * 0.5);
    if width == 0.0 || color.to_rgba()[3] == 0 {
        return Ok(());
    }
    let inset = width / 2.0;
    let shape = RoundedRect::new(
        Rect::new(inset, inset, size.width - width, size.height - width),
        (radius - inset).max(0.0),
    )?;
    builder.stroke(shape, color, width)?;
    Ok(())
}
