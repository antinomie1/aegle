use crate::{
    Result,
    state::{Content, State},
};
use aegle_core::NodeId;
use aegle_scene::{Affine, Rect, RoundedRect};
use aegle_text::EditorPaint;

impl State {
    pub fn record(&mut self, id: NodeId) -> Result {
        let enabled = self.usable(id);
        let element = &mut self.tree.get_mut(id).unwrap().context;
        let theme = self.theme;
        let padding = element.inset(theme.padding);
        let size = element.bounds.size;
        let mut builder = std::mem::take(&mut element.scene).into_builder();
        builder.clear();
        if element.effective_visible {
            match &element.content {
                Content::Label(label) => {
                    builder.push_transform(Affine::translation(padding, padding)?)?;
                    label.paint_with_color(&mut builder, theme.foreground)?;
                    builder.pop()?;
                }
                Content::Button(button, label) => {
                    let shape = RoundedRect::new(
                        Rect::new(
                            0.5,
                            0.5,
                            (size.width - 1.0).max(0.0),
                            (size.height - 1.0).max(0.0),
                        ),
                        theme.radius,
                    )?;
                    let color = if button.is_pressed() {
                        theme.pressed
                    } else if button.is_hovered() {
                        theme.hover
                    } else {
                        theme.surface
                    };
                    builder.fill(shape, color)?;
                    builder.stroke(
                        shape,
                        if button.is_focused() {
                            theme.accent
                        } else {
                            theme.border
                        },
                        1.0,
                    )?;
                    builder.push_clip(shape)?;
                    builder.push_transform(Affine::translation(
                        (size.width - label.size().width) / 2.0,
                        (size.height - label.size().height) / 2.0,
                    )?)?;
                    label.paint_with_color(
                        &mut builder,
                        if enabled {
                            theme.foreground
                        } else {
                            theme.muted
                        },
                    )?;
                    builder.pop()?.pop()?;
                }
                Content::Field(field) => {
                    let shape = RoundedRect::new(
                        Rect::new(
                            0.5,
                            0.5,
                            (size.width - 1.0).max(0.0),
                            (size.height - 1.0).max(0.0),
                        ),
                        theme.radius,
                    )?;
                    builder.fill(shape, theme.surface)?;
                    builder.stroke(
                        shape,
                        if field.is_focused() {
                            theme.accent
                        } else {
                            theme.border
                        },
                        1.0,
                    )?;
                    builder.push_clip(shape)?;
                    builder.push_transform(Affine::translation(
                        padding - element.scroll.x,
                        padding - element.scroll.y,
                    )?)?;
                    field.editor().paint(
                        &mut builder,
                        EditorPaint {
                            foreground: Some(if enabled {
                                theme.foreground
                            } else {
                                theme.muted
                            }),
                            caret: field.is_focused().then_some(theme.accent),
                            preedit: Some(theme.accent),
                            selection: Some(theme.selection),
                            ..Default::default()
                        },
                    )?;
                    builder.pop()?.pop()?;
                }
                Content::Container => {}
            }
        }
        element.scene = builder.finish()?;
        Ok(())
    }
}
