//! The flex item style a container lays out, which may differ from the stored
//! style where the container scrolls.

use taffy::{
    AlignSelf, BoxGenerationMode, BoxSizing, CoreStyle, Dimension, Direction, LengthPercentage,
    LengthPercentageAuto, Overflow, Point, Position, Rect, Size, Style,
    style::{Contain, FlexboxItemStyle},
};

/// An item's style; `fixed` items keep their basis because their container
/// scrolls along its main axis, so content scrolls instead of being compressed.
pub(crate) struct ItemStyle<'a> {
    pub style: &'a Style,
    pub fixed: bool,
}

impl CoreStyle for ItemStyle<'_> {
    type CustomIdent = String;
    fn box_generation_mode(&self) -> BoxGenerationMode {
        self.style.box_generation_mode()
    }
    fn is_block(&self) -> bool {
        self.style.is_block()
    }
    fn is_compressible_replaced(&self) -> bool {
        self.style.is_compressible_replaced()
    }
    fn box_sizing(&self) -> BoxSizing {
        self.style.box_sizing()
    }
    fn direction(&self) -> Direction {
        self.style.direction()
    }
    fn overflow(&self) -> Point<Overflow> {
        self.style.overflow
    }
    fn scrollbar_width(&self) -> f32 {
        self.style.scrollbar_width
    }
    fn position(&self) -> Position {
        self.style.position
    }
    fn inset(&self) -> Rect<LengthPercentageAuto> {
        self.style.inset
    }
    fn size(&self) -> Size<Dimension> {
        self.style.size
    }
    fn min_size(&self) -> Size<LengthPercentageAuto> {
        self.style.min_size
    }
    fn max_size(&self) -> Size<LengthPercentageAuto> {
        self.style.max_size
    }
    fn aspect_ratio(&self) -> Option<f32> {
        self.style.aspect_ratio
    }
    fn margin(&self) -> Rect<LengthPercentageAuto> {
        self.style.margin
    }
    fn padding(&self) -> Rect<LengthPercentage> {
        self.style.padding
    }
    fn border(&self) -> Rect<LengthPercentage> {
        self.style.border
    }
    fn contain(&self) -> Contain {
        self.style.contain()
    }
}

impl FlexboxItemStyle for ItemStyle<'_> {
    fn flex_basis(&self) -> Dimension {
        self.style.flex_basis
    }
    fn flex_grow(&self) -> f32 {
        self.style.flex_grow
    }
    fn flex_shrink(&self) -> f32 {
        if self.fixed {
            0.0
        } else {
            self.style.flex_shrink
        }
    }
    fn align_self(&self) -> Option<AlignSelf> {
        self.style.align_self
    }
}
