//! Unified layout interface abstracting platform-native and fallback engines.

use std::ops::Range;

#[cfg(feature = "scene")]
use aegle_scene::SceneBuilder;
#[cfg(feature = "scene")]
use aegle_types::{Color, Point};

use crate::{Alignment, ContentWidths, TextDiagnostics, style::Shaping};

/// Side of a cluster a caret or selection boundary binds to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Affinity {
    /// Binds to the character after it.
    Downstream,
    /// Binds to the character before it.
    Upstream,
}

/// Geometry of a caret or selection fragment.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoundingBox {
    /// Left coordinate.
    pub x0: f32,
    /// Top coordinate.
    pub y0: f32,
    /// Right coordinate.
    pub x1: f32,
    /// Bottom coordinate.
    pub y1: f32,
}

impl BoundingBox {
    #[allow(dead_code)]
    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    pub fn union(self, other: BoundingBox) -> BoundingBox {
        BoundingBox {
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }
}

/// Layout metrics for one visual line.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LineMetrics {
    /// Starting byte index in the layout's text.
    pub start: usize,
    /// Ending byte index in the layout's text.
    pub end: usize,
    /// Top coordinate relative to the layout origin.
    pub y: f32,
    /// Line height.
    pub height: f32,
    /// Baseline offset relative to the layout origin.
    pub baseline: f32,
}

pub(crate) enum PlatformSystem {
    #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
    Pango(crate::backend_pango::PangoSystem),
    #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
    Parley,
    Stub,
}

impl PlatformSystem {
    pub fn new(system_fonts: bool) -> Self {
        #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
        {
            if let Some(sys) = crate::backend_pango::PangoSystem::new(system_fonts) {
                return Self::Pango(sys);
            }
        }
        #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
        {
            let _ = system_fonts;
            return Self::Parley;
        }
        #[allow(unreachable_code)]
        Self::Stub
    }

    #[allow(unreachable_code, unused_variables, dead_code)]
    pub fn register_fonts(
        &mut self,
        data: &[u8],
    ) -> Result<Vec<crate::FontFamily>, crate::TextError> {
        #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
        if let Self::Pango(sys) = self {
            return sys.register_fonts(data);
        }
        #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
        {
            Ok(Vec::new())
        }
        #[cfg(not(any(feature = "text-parley", not(target_os = "linux"))))]
        Ok(vec![crate::FontFamily(1)])
    }

    #[allow(unreachable_code, unused_variables, dead_code)]
    pub fn set_generic_families(
        &mut self,
        generic: crate::GenericFamily,
        families: &[crate::FontFamily],
    ) {
        #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
        if let Self::Pango(sys) = self {
            sys.set_generic_families(generic, families);
        }
    }

    pub fn trim(&mut self) {
        #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
        if let Self::Pango(sys) = self {
            sys.trim();
        }
    }
}

// The Parley variant is the larger one; layouts are one per paragraph, so the
// difference is not worth boxing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub(crate) enum Layout {
    #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
    Pango(crate::backend_pango::PangoLayout),
    #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
    Parley(crate::backend_parley::ParleyLayout),
    Stub,
}

impl Layout {
    #[allow(dead_code)]
    pub fn new(
        system: &mut PlatformSystem,
        text: &str,
        shaping: &Shaping,
        wrap: bool,
        underline: Option<Range<usize>>,
    ) -> Self {
        #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
        if let PlatformSystem::Pango(sys) = system {
            return Self::Pango(crate::backend_pango::PangoLayout::new(
                sys, text, shaping, wrap, underline,
            ));
        }
        #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
        {
            let _ = (system, text, shaping, wrap, underline);
            return Self::Parley(crate::backend_parley::ParleyLayout::new());
        }
        #[allow(unreachable_code)]
        Self::Stub
    }

    pub fn empty() -> Self {
        Self::Stub
    }

    pub fn reflow(&mut self, width: Option<f32>, alignment: Alignment) {
        match self {
            #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
            Self::Pango(layout) => layout.reflow(width, alignment),
            #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
            Self::Parley(layout) => layout.reflow(width, alignment),
            _ => {}
        }
    }

    pub fn width(&self) -> f32 {
        match self {
            #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
            Self::Pango(layout) => layout.width(),
            #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
            Self::Parley(layout) => layout.width(),
            _ => 0.0,
        }
    }

    pub fn height(&self) -> f32 {
        match self {
            #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
            Self::Pango(layout) => layout.height(),
            #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
            Self::Parley(layout) => layout.height(),
            _ => 0.0,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
            Self::Pango(layout) => layout.line_count(),
            #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
            Self::Parley(layout) => layout.line_count(),
            _ => 0,
        }
    }

    pub fn is_rtl(&self) -> bool {
        match self {
            #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
            Self::Pango(layout) => layout.is_rtl(),
            #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
            Self::Parley(layout) => layout.is_rtl(),
            _ => false,
        }
    }

    pub fn first_baseline(&self) -> Option<f32> {
        match self {
            #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
            Self::Pango(layout) => layout.first_baseline(),
            #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
            Self::Parley(layout) => layout.first_baseline(),
            _ => None,
        }
    }

    pub fn content_widths(&self) -> ContentWidths {
        match self {
            #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
            Self::Pango(layout) => layout.content_widths(),
            #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
            Self::Parley(layout) => layout.content_widths(),
            _ => ContentWidths::default(),
        }
    }

    pub fn line_metrics(&self, line: usize) -> Option<LineMetrics> {
        match self {
            #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
            Self::Pango(layout) => layout.line_metrics(line),
            #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
            Self::Parley(layout) => layout.line_metrics(line),
            _ => None,
        }
    }

    pub fn line_range_from_point(&self, x: f32, y: f32) -> Range<usize> {
        match self {
            #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
            Self::Pango(layout) => layout.line_range_from_point(x, y),
            #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
            Self::Parley(layout) => layout.line_range_from_point(x, y),
            _ => 0..0,
        }
    }

    pub fn line_range_for_index(&self, index: usize) -> Range<usize> {
        match self {
            #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
            Self::Pango(layout) => layout.line_range_for_index(index),
            #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
            Self::Parley(layout) => layout.line_range_for_index(index),
            _ => 0..0,
        }
    }

    pub fn hit_test(&self, x: f32, y: f32) -> (usize, Affinity) {
        match self {
            #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
            Self::Pango(layout) => layout.hit_test(x, y),
            #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
            Self::Parley(layout) => layout.hit_test(x, y),
            _ => (0, Affinity::Downstream),
        }
    }

    pub fn caret_geometry(&self, index: usize, affinity: Affinity, width: f32) -> BoundingBox {
        match self {
            #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
            Self::Pango(layout) => layout.caret_geometry(index, affinity, width),
            #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
            Self::Parley(layout) => layout.caret_geometry(index, affinity, width),
            _ => BoundingBox::default(),
        }
    }

    pub fn move_cursor(
        &self,
        index: usize,
        affinity: Affinity,
        forward: bool,
    ) -> (usize, Affinity) {
        match self {
            #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
            Self::Pango(layout) => layout.move_cursor(index, affinity, forward),
            #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
            Self::Parley(layout) => layout.move_cursor(index, affinity, forward),
            _ => (index, affinity),
        }
    }

    pub fn range_boxes(&self, range: Range<usize>, visit: impl FnMut(BoundingBox)) {
        match self {
            #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
            Self::Pango(layout) => layout.range_boxes(range, visit),
            #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
            Self::Parley(layout) => layout.range_boxes(range, visit),
            _ => {}
        }
    }

    pub fn grapheme_range(&self, line_text: &str, caret: usize, previous: bool) -> Range<usize> {
        match self {
            #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
            Self::Pango(layout) => layout.grapheme_range(line_text, caret, previous),
            #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
            Self::Parley(layout) => layout.grapheme_range(line_text, caret, previous),
            _ => caret..caret,
        }
    }

    pub fn diagnostics(&self, text: &str) -> TextDiagnostics {
        match self {
            #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
            Self::Pango(layout) => layout.diagnostics(text),
            #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
            Self::Parley(layout) => layout.diagnostics(text),
            _ => TextDiagnostics::default(),
        }
    }

    #[cfg(feature = "scene")]
    pub fn paint(&self, builder: &mut SceneBuilder, color: Color, weight: f32, origin: Point) {
        match self {
            #[cfg(all(target_os = "linux", not(feature = "text-parley")))]
            Self::Pango(layout) => layout.paint(builder, color, weight, origin),
            #[cfg(any(feature = "text-parley", not(target_os = "linux")))]
            Self::Parley(layout) => layout.paint(builder, color, weight, origin),
            _ => {}
        }
    }
}
