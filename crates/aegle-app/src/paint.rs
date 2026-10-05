use crate::{
    Result,
    state::{Content, State},
};
use aegle_core::NodeId;
use aegle_scene::{Affine, Color, Rect, RoundedRect, SceneBuilder};
use aegle_text::EditorPaint;
use aegle_types::Size;

impl State {
    pub fn record(&mut self, id: NodeId) -> Result {
        let visual = self.visual_state(id);
        let appearance = self.appearance_for(id, visual)?;
        #[cfg(feature = "motion")]
        let appearance = self.transition_appearance(id, appearance)?;
        let element = &mut self.tree.get_mut(id).unwrap().context;
        let padding = element.inset(self.theme.padding);
        let size = element.bounds.size;
        let mut builder = std::mem::take(&mut element.scene).into_builder();
        builder.clear();
        if element.effective_visible {
            let range = matches!(element.content, Content::Slider(_) | Content::Progress(_));
            let shape = RoundedRect::new(
                Rect::new(0.0, 0.0, size.width, size.height),
                appearance.radius,
            )?;
            if !range && appearance.background.to_rgba()[3] != 0 {
                builder.fill(shape, appearance.background)?;
            }
            if !range && !matches!(element.content, Content::Toggle(_)) {
                outline(
                    &mut builder,
                    size,
                    appearance.radius,
                    appearance.border_width,
                    appearance.border_color,
                )?;
            }
            match &element.content {
                Content::Label(label) => {
                    builder.push_transform(Affine::translation(padding, padding)?)?;
                    label.paint_with_color(&mut builder, appearance.foreground)?;
                    builder.pop()?;
                }
                Content::Button(_, label) => {
                    builder.push_clip(shape)?;
                    builder.push_transform(Affine::translation(
                        (size.width - label.size().width) / 2.0,
                        (size.height - label.size().height) / 2.0,
                    )?)?;
                    label.paint_with_color(&mut builder, appearance.foreground)?;
                    builder.pop()?.pop()?;
                }
                Content::Field(field) => {
                    builder.push_clip(shape)?;
                    builder.push_transform(Affine::translation(
                        padding - element.scroll.x,
                        padding - element.scroll.y,
                    )?)?;
                    field.editor().paint(
                        &mut builder,
                        EditorPaint {
                            foreground: Some(appearance.foreground),
                            caret: visual.focused.then_some(appearance.caret),
                            preedit: Some(appearance.caret),
                            selection: Some(appearance.selection),
                            ..Default::default()
                        },
                    )?;
                    builder.pop()?.pop()?;
                }
                Content::Toggle(toggle) => crate::widget_paint::toggle(
                    &mut builder,
                    size,
                    padding,
                    self.theme.gap,
                    toggle.switch,
                    toggle.control.is_checked(),
                    &toggle.text,
                    appearance,
                )?,
                Content::Slider(slider) => crate::widget_paint::range(
                    &mut builder,
                    size,
                    padding,
                    slider.range().fraction(),
                    true,
                    appearance,
                )?,
                Content::Progress(range) => crate::widget_paint::range(
                    &mut builder,
                    size,
                    padding,
                    range.fraction(),
                    false,
                    appearance,
                )?,
                Content::Container | Content::Scroll => {}
            }
            outline(
                &mut builder,
                size,
                appearance.radius,
                appearance.focus_width,
                appearance.focus_color,
            )?;
        }
        element.scene = builder.finish()?;
        Ok(())
    }
}

// Keep centered scene strokes inside the logical bounds. Large requested widths
// saturate at half the smaller extent, preserving a drawable center rectangle.
fn outline(
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
