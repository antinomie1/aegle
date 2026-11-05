//! Display text.

use std::cell::RefCell;

use aegle_scene::Affine;
use aegle_text::{Paragraph, TextSystem};
use aegle_theme::{ControlKind, Theme};
use aegle_types::Size;
use aegle_ui::LayoutStyle;
use aegle_ui::{
    Container, Control, OrFail, Result,
    control::{MeasureCx, PaintCx},
    handle, text_style,
};

handle! {
    /// A retained display paragraph.
    pub Label(LabelControl): text
}

impl Label {
    /// Replaces text and invalidates its shared layout, scene and semantic state.
    pub fn set_text(&self, text: &str) {
        self.change(|state, id| state.set_text(id, text))
    }
    /// Copies the current display text.
    pub fn text(&self) -> String {
        self.change(|state, id| state.text(id))
    }
    /// Sets whether the label wraps text when its line overflows available width.
    ///
    /// When `wrap` is enabled, text wraps to available layout width and flex shrink
    /// is set to 1.0. When disabled (the default), text stays on a single line
    /// with flex shrink 0.0.
    pub fn set_wrap(&self, wrap: bool) {
        self.update(|control| control.wrap = wrap);
        self.set_shrink(if wrap { 1.0 } else { 0.0 });
    }
    /// Whether text wrapping is enabled.
    pub fn is_wrap(&self) -> bool {
        self.read(|control| control.wrap)
    }
    /// Whether the last layout left it narrower than its single line, so it
    /// shows a prefix ending in "…"; accessibility still reads the whole text.
    pub fn is_elided(&self) -> bool {
        self.read(|control| control.elided.is_some())
    }
}

/// The control inside a [`Label`] node.
pub struct LabelControl {
    text: Paragraph,
    /// What a single line narrower than its text shows instead.
    elided: Option<Elided>,
    pub(crate) wrap: bool,
    /// [`crate::kinds::LABEL`], or the kind of a label-like part such as a tooltip.
    pub(crate) kind: &'static ControlKind,
}

impl LabelControl {
    /// A label showing `text`, styled by `theme`.
    pub fn new(fonts: &RefCell<TextSystem>, text: &str, theme: &Theme) -> Result<Self> {
        Ok(Self {
            text: fonts.borrow_mut().paragraph(text, &text_style(theme))?,
            elided: None,
            wrap: false,
            kind: &crate::kinds::LABEL,
        })
    }
}

/// A label's text cut to fit its width, ending in an ellipsis.
struct Elided {
    /// The width and text it was cut for.
    room: f32,
    source: String,
    shown: Paragraph,
}

impl LabelControl {
    /// Keeps the longest prefix that fits with "…" while a single line is
    /// wider than the label, as when a flexible row shrinks it.
    fn elide(&mut self, cx: &MeasureCx<'_>) {
        let room = cx.width.map(|width| (width - 2.0 * cx.padding).max(0.0));
        let Some(room) = room.filter(|room| !self.wrap && self.text.size().width > room + 0.5)
        else {
            self.elided = None;
            return;
        };
        let source = self.text.text();
        if self
            .elided
            .as_ref()
            .is_some_and(|e| e.room == room && e.source == source)
        {
            return;
        }
        let ends: Vec<usize> = source.char_indices().map(|(at, _)| at).collect();
        let fonts = &mut cx.fonts.borrow_mut();
        let cut = |fonts: &mut TextSystem, kept: usize| {
            let text = format!("{}…", source[..ends[kept]].trim_end());
            fonts.derive(&self.text, text).or_fail()
        };
        // Binary search on kept characters: none always shows, all never fits.
        let (mut fits, mut over) = (0, ends.len());
        let mut shown = cut(fonts, 0);
        while over - fits > 1 {
            let middle = (fits + over) / 2;
            let candidate = cut(fonts, middle);
            if candidate.size().width <= room + 0.5 {
                (fits, shown) = (middle, candidate);
            } else {
                over = middle;
            }
        }
        let source = source.to_owned();
        self.elided = Some(Elided { room, source, shown });
    }
}

impl Control for LabelControl {
    fn kind(&self) -> &'static ControlKind {
        self.kind
    }
    fn default_padding(&self, _: &Theme) -> f32 {
        0.0
    }
    fn paragraph(&self) -> Option<&Paragraph> {
        Some(&self.text)
    }
    fn paragraph_mut(&mut self) -> Option<&mut Paragraph> {
        Some(&mut self.text)
    }
    fn baseline(&self, _: Size, padding: f32) -> Option<f32> {
        Some(padding + self.text.first_baseline()?)
    }
    fn measure(&mut self, cx: &MeasureCx<'_>) -> Size {
        let width = if self.wrap { cx.content_width() } else { None };
        let size = self.text.reflow(width, cx.alignment()).or_fail();
        let mut outer = size.width + 2.0 * cx.padding;
        // A single line narrower than its text elides, never drawing past the
        // width it is offered; reporting its whole text there would make a
        // scroll view scroll sideways to nothing. 0 asks for the min-content
        // width, which stays the whole line.
        if let Some(offered) = cx.width.filter(|&w| !self.wrap && w > 0.0) {
            outer = outer.min(offered);
        }
        Size::new(outer, size.height + 2.0 * cx.padding)
    }
    fn finalize(&mut self, cx: &MeasureCx<'_>) {
        let width = if self.wrap { cx.content_width() } else { None };
        self.text.reflow(width, cx.alignment()).or_fail();
        self.elide(cx);
    }
    fn restyle(&mut self, fonts: &mut TextSystem, style: &aegle_text::TextStyle<'_>) {
        fonts.restyle(&mut self.text, style).or_fail();
        self.elided = None;
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        cx.builder
            .push_transform(Affine::translation(cx.padding, cx.padding));
        let shown = self.elided.as_ref().map_or(&self.text, |e| &e.shown);
        shown.paint_with_color(cx.builder, cx.appearance.foreground);
        cx.builder.pop();
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        cx.set_role(aegle_ui::Role::Label);
        cx.set_value(self.text.text());
    }
}

pub(crate) fn create(container: &Container, text: &str) -> Label {
    Label(crate::add(container, |state, theme| {
        Ok((Box::new(LabelControl::new(state.fonts(), text, theme)?), {
            let mut style = LayoutStyle::default();
            style.set_shrink(0.0);
            style
        }))
    }))
}
