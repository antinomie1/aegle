use aegle_scene::{RoundedRect, SceneBuilder};
use aegle_types::{Color, Rect};
use parley::{Affinity, Cursor, Selection};

use crate::{Editor, editor::to_rect, paint::paint_layout};

/// Colors and caret width for recording an editor's current visual state.
///
/// Hosts control focus and caret blinking by omitting decoration colors. There
/// is no timer or independent selection state in the painting path. Positions
/// and widths are logical pixels; the host supplies translation and clipping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorPaint {
    /// Overrides glyph foreground without reshaping; `None` uses stored styles.
    pub foreground: Option<Color>,
    /// Selection background; `None` omits the highlight.
    pub selection: Option<Color>,
    /// Caret color; `None` hides it. An IME-hidden caret stays hidden regardless.
    pub caret: Option<Color>,
    /// One-pixel preedit underline; `None` omits it.
    pub preedit: Option<Color>,
    /// Finite, positive caret width, even when the caret color is `None`.
    pub caret_width: f32,
}

impl Default for EditorPaint {
    fn default() -> Self {
        Self {
            foreground: None,
            selection: Some(Color::rgba(71, 115, 184, 64)),
            caret: Some(Color::BLACK),
            preedit: Some(Color::BLACK),
            caret_width: 1.0,
        }
    }
}

impl Editor {
    /// Appends selection, glyphs, preedit underline and caret in drawing order.
    ///
    /// All geometry comes from the current layout, including wrapped or
    /// bidirectional composition. This never reshapes or mutates editing state.
    /// Focus and blink visibility are controlled by [`EditorPaint`]. The caller
    /// supplies the same transform and clip used for hit testing and IME bounds.
    ///
    /// Panics unless the caret width is finite and positive, and as
    /// [`crate::Paragraph::paint`] does.
    pub fn paint(&self, builder: &mut SceneBuilder, paint: EditorPaint) {
        let caret = self
            .caret_rect(paint.caret_width)
            .expect("caret width must be finite and positive");
        if let Some(color) = paint.selection {
            self.selection_rects(|rect| fill_rect(builder, rect, color));
        }
        paint_layout(
            self.layout(),
            self.diagnostics(),
            builder,
            paint.foreground,
            self.weight,
        );

        if let (Some(range), Some(color)) = (self.composition_range(), paint.preedit) {
            let layout = self.layout();
            let selection = Selection::new(
                Cursor::from_byte_index(layout, range.start, Affinity::Downstream),
                Cursor::from_byte_index(layout, range.end, Affinity::Upstream),
            );
            selection.geometry_with(layout, |bounds, _| {
                let rect = to_rect(bounds);
                let underline = Rect::new(
                    rect.origin.x,
                    rect.origin.y + rect.size.height - 1.0,
                    rect.size.width,
                    1.0,
                );
                fill_rect(builder, underline, color);
            });
        }

        if let (Some(rect), Some(color)) = (caret, paint.caret) {
            fill_rect(builder, rect, color);
        }
    }
}

fn fill_rect(builder: &mut SceneBuilder, rect: Rect, color: Color) {
    builder.fill(RoundedRect::new(rect, 0.0), color);
}
