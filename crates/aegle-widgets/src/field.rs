//! Single-line and multiline plain text editors.

use aegle_controls::{Input, Outcome};
use aegle_scene::Affine;
use aegle_text::{EditorDriver, EditorOptions, EditorPaint, Selection, TextError, TextSystem};
use aegle_theme::{ControlKind, Theme};
use aegle_types::{Point, Size};
use aegle_ui::LayoutStyle;
use aegle_ui::{
    Container, Control, HandlerResult, OrFail, Result, bar,
    control::{ControlVisual, InputCx, MeasureCx, PaintCx},
    handle, text_style,
};

handle! {
    /// A retained plain text editor, including native IME composition state.
    pub TextField(FieldControl): text, interactive, editor
}

impl TextField {
    /// Replaces the committed text, clears history and explicitly ends native preedit.
    pub fn set_text(&self, text: &str) {
        self.change(|state, id| state.set_text(id, text))
    }
    /// Copies committed text, never substituting transient preedit.
    pub fn text(&self) -> String {
        self.change(|state, id| state.text(id))
    }
    /// Allows selection but rejects user edits when true.
    pub fn set_read_only(&self, read_only: bool) {
        self.edit(|editor| {
            editor.set_read_only(read_only);
            Ok(())
        })
    }
    /// Masks the value with one bullet per character. Password fields keep no
    /// undo history, open no IME composition, refuse copy/cut and expose only
    /// the masks to accessibility. Selections then use display-buffer offsets.
    pub fn set_password(&self, password: bool) {
        self.edit(|editor| {
            editor.set_password(password);
            Ok(())
        })
    }
    /// Changes the committed UTF-8 selection, cancelling active preedit like
    /// [`Self::set_text`]. Panics on endpoints outside the text or inside a
    /// character.
    pub fn select(&self, selection: Selection) {
        self.edit(|editor| {
            editor.cancel_preedit();
            editor.select(selection)
        })
    }
    /// Adds a single-line Enter handler, dispatched outside the tree borrow.
    pub fn on_submit<R: HandlerResult>(
        &self,
        mut callback: impl FnMut(TextField) -> R + 'static,
    ) -> aegle_ui::Subscription {
        self.on_action(move |node| callback(TextField(node)).into_result())
    }
    /// Adds a handler run after each user edit of the committed text: typing,
    /// deletion, paste, cut, undo, redo or an IME commit. Preedit and
    /// [`Self::set_text`] do not invoke it. Handlers run in registration order
    /// outside UI borrows.
    pub fn on_change<R: HandlerResult>(
        &self,
        mut callback: impl FnMut(TextField) -> R + 'static,
    ) -> aegle_ui::Subscription {
        self.change(|state, id| {
            Ok(state.on_edit(id, move |node| callback(TextField(node)).into_result()))
        })
    }
    /// Applies an editor change, then restarts a focused native IME session.
    fn edit(
        &self,
        apply: impl FnOnce(&mut EditorDriver<'_>) -> std::result::Result<(), TextError>,
    ) {
        self.change(|state, id| {
            let fonts = state.fonts().clone();
            let field = state
                .element_mut(id)
                .control_mut()
                .editor_mut()
                .expect("a text field node holds an editor");
            apply(&mut fonts.borrow_mut().edit(field.editor_mut()))?;
            if state.focused() == Some(id) {
                state.reset_ime();
            }
            Ok(())
        })
    }
}

/// The control inside a [`TextField`] node.
pub struct FieldControl(pub(crate) Box<aegle_controls::TextField>);

impl FieldControl {
    /// Where the text layout starts in the control's local coordinates.
    ///
    /// A single line is centered vertically in the control, like a button's
    /// label; a multiline area starts at the top padding and scrolls.
    fn text_origin(&self, size: Size, padding: f32, scroll: Point) -> Point {
        let editor = self.0.editor();
        let y = match editor.is_multiline() {
            true => padding - scroll.y,
            false => (size.height - editor.size().height) * 0.5,
        };
        Point::new(padding - scroll.x, y)
    }
}

impl Control for FieldControl {
    fn kind(&self) -> &'static ControlKind {
        &crate::kinds::TEXT_FIELD
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
    fn handle(&mut self, cx: &mut InputCx<'_>, input: Input<'_>) -> Outcome {
        self.0.handle(cx.fonts, input).or_fail()
    }
    fn content_offset(&self, size: Size, padding: f32, scroll: Point) -> Point {
        let origin = self.text_origin(size, padding, scroll);
        Point::new(-origin.x, -origin.y)
    }
    fn baseline(&self, size: Size, padding: f32) -> Option<f32> {
        // The unscrolled text origin, as CSS aligns scroll containers.
        let origin = self.text_origin(size, padding, Point::default());
        Some(origin.y + self.0.editor().first_baseline()?)
    }
    fn measure(&mut self, cx: &MeasureCx<'_>) -> Size {
        let size = cx
            .fonts
            .borrow_mut()
            .edit(self.0.editor_mut())
            .reflow(cx.content_width(), cx.alignment())
            .or_fail();
        Size::new(
            size.width + 2.0 * cx.padding,
            size.height + 2.0 * cx.padding,
        )
    }
    fn finalize(&mut self, cx: &MeasureCx<'_>) {
        cx.fonts
            .borrow_mut()
            .edit(self.0.editor_mut())
            .reflow(cx.content_width(), cx.alignment())
            .or_fail();
    }
    fn retheme(
        &self,
        theme: &Theme,
        local: aegle_ui::LocalLayout,
        _: bool,
        style: &mut LayoutStyle,
    ) {
        if !local.contains(aegle_ui::LocalLayout::HEIGHT) {
            let lines = if self.0.editor().is_multiline() {
                4.0
            } else {
                1.0
            };
            style.set_height(theme.control_height * lines);
        }
        if !local.contains(aegle_ui::LocalLayout::MIN_HEIGHT) {
            style.set_min_height(theme.control_height);
        }
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let origin = self.text_origin(cx.size, cx.padding, cx.scroll);
        cx.builder.push_clip(cx.shape);
        cx.builder
            .push_transform(Affine::translation(origin.x, origin.y));
        self.0.editor().paint(
            cx.builder,
            EditorPaint {
                foreground: Some(cx.appearance.foreground),
                caret: cx.visual.focused.then_some(cx.appearance.caret),
                preedit: Some(cx.appearance.caret),
                selection: Some(cx.appearance.selection),
                ..Default::default()
            },
        );
        cx.builder.pop().pop();
        bar::paint(cx.builder, cx.bars, cx.bar_color);
    }
}

/// A field control and its default layout style under `theme`.
pub(crate) fn control(
    state: &mut aegle_ui::State,
    theme: &Theme,
    text: &str,
    multiline: bool,
) -> Result<(FieldControl, LayoutStyle)> {
    let editor = state.fonts().borrow_mut().editor(
        text,
        &text_style(theme),
        EditorOptions {
            multiline,
            ..Default::default()
        },
    )?;
    Ok((
        FieldControl(Box::new(aegle_controls::TextField::new(editor))),
        {
            let lines = if multiline { 4.0 } else { 1.0 };
            let mut style = LayoutStyle::default();
            style
                .set_height(theme.control_height * lines)
                .set_min_width(0.0)
                .set_min_height(theme.control_height);
            style
        },
    ))
}

pub(crate) fn create(container: &Container, text: &str, multiline: bool) -> TextField {
    TextField(crate::add(container, |state, theme| {
        let (control, style) = control(state, theme, text, multiline)?;
        Ok((Box::new(control) as Box<dyn Control>, style))
    }))
}
