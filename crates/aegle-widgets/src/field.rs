//! Single-line and multiline plain text editors.

use std::any::Any;

use aegle_controls::{Input, Outcome};
use aegle_layout::{Dimension, LengthPercentageAuto, Style};
use aegle_scene::Affine;
use aegle_text::{EditorDriver, EditorOptions, EditorPaint, Selection, TextError, TextSystem};
use aegle_theme::{ControlKind, Theme};
use aegle_types::{Point, Size};
use aegle_ui::{
    Container, Control, Result, bar,
    control::{ControlVisual, InputCx, MeasureCx, PaintCx},
    handle, text_style,
};

handle!(
    TextField,
    "A retained plain text editor, including native IME composition state."
);

impl TextField {
    /// Replaces the committed text, clears history and explicitly ends native preedit.
    pub fn set_text(&self, text: &str) -> Result {
        self.0.set_text(text)
    }
    /// Copies committed text, never substituting transient preedit.
    pub fn text(&self) -> Result<String> {
        self.0.text()
    }
    /// Allows selection but rejects user edits when true.
    pub fn set_read_only(&self, read_only: bool) -> Result {
        self.edit(|editor| {
            editor.set_read_only(read_only);
            Ok(())
        })
    }
    /// Masks the value with one bullet per character. Password fields keep no
    /// undo history, open no IME composition, refuse copy/cut and expose only
    /// the masks to accessibility. Selections then use display-buffer offsets.
    pub fn set_password(&self, password: bool) -> Result {
        self.edit(|editor| {
            editor.set_password(password);
            Ok(())
        })
    }
    /// Changes the committed UTF-8 selection; active preedit must first be cancelled.
    pub fn select(&self, selection: Selection) -> Result {
        self.edit(|editor| editor.select(selection))
    }
    /// Replaces the single-line Enter handler, dispatched outside the tree borrow.
    pub fn on_submit(&self, mut callback: impl FnMut(TextField) -> Result + 'static) -> Result {
        self.0.on_action(move |node| callback(TextField(node)))
    }
    /// Removes the submit handler and any queued invocation.
    pub fn clear_on_submit(&self) -> Result {
        self.0.clear_on_action()
    }
    /// Applies an editor change, then restarts a focused native IME session.
    fn edit(
        &self,
        apply: impl FnOnce(&mut EditorDriver<'_>) -> std::result::Result<(), TextError>,
    ) -> Result {
        self.change(|state, id| {
            let fonts = state.fonts.clone();
            let field = state
                .tree
                .get_mut(id)
                .unwrap()
                .context
                .control
                .editor_mut()
                .expect("a text field node holds an editor");
            apply(&mut fonts.borrow_mut().edit(field.editor_mut()))?;
            if state.focus.current(&state.tree) == Some(id) {
                state.ime_dirty = true;
                state.ime_reset = true;
                state.input_method = false;
            }
            Ok(())
        })
    }
}

/// The control inside a [`TextField`] node.
pub struct FieldControl(pub(crate) Box<aegle_controls::TextField>);

impl Control for FieldControl {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn kind(&self) -> ControlKind {
        ControlKind::TextField
    }
    fn interactive(&self) -> bool {
        true
    }
    fn drags(&self) -> bool {
        true
    }
    fn self_clipping(&self) -> bool {
        true
    }
    fn editor(&self) -> Option<&aegle_controls::TextField> {
        Some(&self.0)
    }
    fn editor_mut(&mut self) -> Option<&mut aegle_controls::TextField> {
        Some(&mut self.0)
    }
    fn visual(&self) -> ControlVisual {
        ControlVisual {
            read_only: self.0.editor().is_read_only(),
            ..Default::default()
        }
    }
    fn set_enabled(&mut self, fonts: &mut TextSystem, enabled: bool) -> Outcome {
        self.0.set_enabled(fonts, enabled)
    }
    fn handle(&mut self, cx: &mut InputCx<'_>, input: Input<'_>) -> Result<Outcome> {
        Ok(self.0.handle(cx.fonts, input)?)
    }
    fn content_offset(&self, _: Size, padding: f32, scroll: Point) -> Point {
        Point::new(scroll.x - padding, scroll.y - padding)
    }
    fn baseline(&self, _: Size, padding: f32) -> Option<f32> {
        // The unscrolled text origin, as CSS aligns scroll containers.
        Some(padding + self.0.editor().first_baseline()?)
    }
    fn measure(&mut self, cx: &MeasureCx<'_>) -> Result<Size> {
        let size = cx
            .fonts
            .borrow_mut()
            .edit(self.0.editor_mut())
            .reflow(cx.content_width(), cx.alignment())?;
        Ok(Size::new(
            size.width + 2.0 * cx.padding,
            size.height + 2.0 * cx.padding,
        ))
    }
    fn finalize(&mut self, cx: &MeasureCx<'_>) -> Result {
        cx.fonts
            .borrow_mut()
            .edit(self.0.editor_mut())
            .reflow(cx.content_width(), cx.alignment())?;
        Ok(())
    }
    fn retheme(&self, theme: &Theme, local: aegle_ui::LocalLayout, _: bool, style: &mut Style) {
        if !local.contains(aegle_ui::LocalLayout::HEIGHT) {
            let lines = if self.0.editor().is_multiline() {
                4.0
            } else {
                1.0
            };
            style.size.height = Dimension::length(theme.control_height * lines);
        }
        if !local.contains(aegle_ui::LocalLayout::MIN_HEIGHT) {
            style.min_size.height = LengthPercentageAuto::length(theme.control_height);
        }
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) -> Result {
        cx.builder.push_clip(cx.shape)?;
        cx.builder.push_transform(Affine::translation(
            cx.padding - cx.scroll.x,
            cx.padding - cx.scroll.y,
        )?)?;
        self.0.editor().paint(
            cx.builder,
            EditorPaint {
                foreground: Some(cx.appearance.foreground),
                caret: cx.visual.focused.then_some(cx.appearance.caret),
                preedit: Some(cx.appearance.caret),
                selection: Some(cx.appearance.selection),
                ..Default::default()
            },
        )?;
        cx.builder.pop()?.pop()?;
        bar::paint(cx.builder, cx.bars, cx.bar_color, cx.theme.radius)?;
        Ok(())
    }
}

/// A field control and its default layout style under `theme`.
pub(crate) fn control(
    state: &mut aegle_ui::State,
    theme: &Theme,
    text: &str,
    multiline: bool,
) -> Result<(FieldControl, Style)> {
    let editor = state.fonts.borrow_mut().editor(
        text,
        &text_style(theme),
        EditorOptions {
            multiline,
            ..Default::default()
        },
    )?;
    Ok((
        FieldControl(Box::new(aegle_controls::TextField::new(editor))),
        Style {
            size: aegle_layout::Size {
                width: Dimension::auto(),
                height: Dimension::length(theme.control_height * if multiline { 4.0 } else { 1.0 }),
            },
            min_size: aegle_layout::Size {
                width: LengthPercentageAuto::length(0.0),
                height: LengthPercentageAuto::length(theme.control_height),
            },
            flex_shrink: 0.0,
            ..Default::default()
        },
    ))
}

pub(crate) fn create(container: &Container, text: &str, multiline: bool) -> Result<TextField> {
    crate::add(container, |state, theme| {
        let (control, style) = control(state, theme, text, multiline)?;
        Ok((Box::new(control) as Box<dyn Control>, style))
    })
    .map(TextField)
}
