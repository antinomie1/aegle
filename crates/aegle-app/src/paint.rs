use crate::{
    Result,
    state::{Content, Semantic, State},
};
use aegle_core::NodeId;
use aegle_scene::{Affine, Color, Rect, RoundedRect, SceneBuilder};
use aegle_text::EditorPaint;
use aegle_types::Size;
use aegle_widgets::{CHEVRON, ToggleSpec, check_mark, chevron};

impl State {
    pub fn record(&mut self, id: NodeId) -> Result {
        let visual = self.visual_state(id);
        let appearance = self.appearance_for(id, visual)?;
        #[cfg(feature = "motion")]
        let appearance = self.transition_appearance(id, appearance)?;
        let bars = self.scrollbars(id);
        let bar_color = self.scrollbar_color(id);
        let theme = *self.theme_of(id);
        let chosen = self.option_selected(id) == Some(true);
        let element = &mut self.tree.get_mut(id).unwrap().context;
        let padding = element.inset(&theme);
        let size = element.bounds.size;
        let semantic = element.semantic;
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
            // A viewport draws its border after its children, so content scrolled
            // under the edge cannot cover it.
            if !range && !matches!(element.content, Content::Toggle(_) | Content::Scroll(_)) {
                outline(
                    &mut builder,
                    size,
                    appearance.radius,
                    appearance.border_width,
                    appearance.border_color,
                )?;
            }
            match &mut element.content {
                Content::Label(label) => {
                    builder.push_transform(Affine::translation(padding, padding)?)?;
                    label.paint_with_color(&mut builder, appearance.foreground)?;
                    builder.pop()?;
                }
                Content::Button(_, label) => {
                    builder.push_clip(shape)?;
                    // Dropdowns and their choices read as lists: text starts at the padding.
                    let listed = matches!(semantic, Semantic::Dropdown | Semantic::Option);
                    let x = if listed {
                        padding
                    } else {
                        (size.width - label.size().width) / 2.0
                    };
                    builder.push_transform(Affine::translation(
                        x,
                        (size.height - label.size().height) / 2.0,
                    )?)?;
                    label.paint_with_color(&mut builder, appearance.foreground)?;
                    builder.pop()?;
                    if semantic == Semantic::Dropdown {
                        let x = size.width - padding - CHEVRON * 0.5;
                        chevron(&mut builder, x, size.height * 0.5, appearance.foreground)?;
                    }
                    if chosen {
                        // The current choice is marked by shape, not only color.
                        let mark = 12.0_f32.min(size.height);
                        let (x, y) = (size.width - padding - mark, (size.height - mark) * 0.5);
                        check_mark(&mut builder, x, y, mark, appearance.indicator)?;
                    }
                    builder.pop()?;
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
                    aegle_widgets::scrollbar::paint(&mut builder, bars, bar_color, theme.radius)?;
                }
                Content::Toggle(toggle) => aegle_widgets::toggle(
                    &mut builder,
                    &ToggleSpec {
                        size,
                        padding,
                        gap: theme.gap,
                        mark: toggle.mark,
                        checked: toggle.control.is_checked(),
                        mixed: toggle.mixed,
                        label_height: (!toggle.text.text().is_empty())
                            .then(|| toggle.text.size().height),
                    },
                    appearance,
                    |builder, color| toggle.text.paint_with_color(builder, color),
                )?,
                Content::Slider(slider) => aegle_widgets::range(
                    &mut builder,
                    size,
                    padding,
                    slider.range().fraction(),
                    true,
                    appearance,
                )?,
                Content::Progress(range) => aegle_widgets::range(
                    &mut builder,
                    size,
                    padding,
                    range.fraction(),
                    false,
                    appearance,
                )?,
                Content::Image(image) => {
                    builder.image(image, Rect::new(0.0, 0.0, size.width, size.height))?;
                }
                Content::Canvas(painter) => painter(&mut builder, size)?,
                Content::Container | Content::Scroll(_) => {}
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
        if let Content::Scroll(overlay) = &mut element.content {
            let mut builder = std::mem::take(&mut **overlay).into_builder();
            builder.clear();
            if element.effective_visible {
                outline(
                    &mut builder,
                    size,
                    appearance.radius,
                    appearance.border_width,
                    appearance.border_color,
                )?;
            }
            aegle_widgets::scrollbar::paint(&mut builder, bars, bar_color, theme.radius)?;
            **overlay = builder.finish()?;
        }
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
