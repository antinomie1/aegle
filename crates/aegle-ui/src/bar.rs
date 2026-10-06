//! Overlay scrollbar geometry and painting.
//!
//! Bars take no layout space: while an axis overflows, a light track spans the
//! viewport's bottom or end edge (right, or left right to left) and a darker
//! square thumb moves along it. Pressing the
//! thumb drags it; pressing elsewhere on the strip centers the thumb there and
//! keeps dragging. No timer, fade or hover animation is involved.
use aegle_scene::{Color, Rect, RoundedRect, SceneBuilder, SceneError};
use aegle_types::{Point, Size};

/// Pointer strip width along a scrollable edge, in logical pixels.
pub const STRIP: f32 = 12.0;
/// Visible track and thumb thickness at the outer edge of the strip.
pub const THICKNESS: f32 = 8.0;
const MARGIN: f32 = 2.0;
/// Gap kept between content and a bar, so controls never touch the track.
const CLEARANCE: f32 = 4.0;
/// Space a viewport must leave beyond its content on the bar's side: the track
/// thickness, its margin from the edge and a clearance. With at least this much
/// trailing padding no control sits under or against a bar.
pub const FOOTPRINT: f32 = THICKNESS + MARGIN + CLEARANCE;
const MIN_THUMB: f32 = 24.0;

/// One overflowing axis in the viewport's local coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bar {
    /// Whether this bar scrolls vertically.
    pub vertical: bool,
    /// Pointer-sensitive strip along the viewport edge.
    pub strip: Rect,
    /// Thumb offset along the strip.
    pub thumb: f32,
    /// Thumb length along the strip.
    pub length: f32,
    /// Laid out right to left: a vertical bar is on the left edge, and a
    /// horizontal thumb starts at the right end.
    pub rtl: bool,
}

impl Bar {
    /// The vertical then horizontal bar of a viewport of `size` scrolled to
    /// `offset`, where `limit` is the maximum scroll on each axis. A bar exists for
    /// an axis whose limit is positive; `horizontal_allowed` is false for
    /// multiline editors, which only scroll vertically. Offsets are measured from
    /// the start edge, so right to left a zero horizontal offset puts the thumb
    /// at the right end.
    pub fn layout(
        size: Size,
        offset: Point,
        limit: Point,
        horizontal_allowed: bool,
        rtl: bool,
    ) -> [Option<Self>; 2] {
        let vertical = limit.y > 0.0;
        let horizontal = horizontal_allowed && limit.x > 0.0;
        let bar = |vertical: bool, track: f32, cross: f32, viewport: f32, offset: f32, max: f32| {
            // Both ends stay clear of the viewport's border.
            let track = (track - 2.0 * MARGIN).max(0.0);
            let width = STRIP.min(cross.max(0.0));
            let length = (track * viewport / (viewport + max))
                .max(MIN_THUMB)
                .min(track);
            let fraction = (offset / max).clamp(0.0, 1.0);
            Self {
                vertical,
                strip: if vertical {
                    Rect::new(if rtl { 0.0 } else { cross - width }, MARGIN, width, track)
                } else {
                    // Right to left the vertical bar's corner is on the left.
                    let start = MARGIN
                        + if rtl {
                            size.width - 2.0 * MARGIN - track
                        } else {
                            0.0
                        };
                    Rect::new(start, cross - width, track, width)
                },
                thumb: if rtl && !vertical {
                    1.0 - fraction
                } else {
                    fraction
                } * (track - length),
                length,
                rtl,
            }
        };
        let corner = |other: bool| if other { STRIP } else { 0.0 };
        [
            vertical.then(|| {
                bar(
                    true,
                    size.height - corner(horizontal),
                    size.width,
                    size.height,
                    offset.y,
                    limit.y,
                )
            }),
            horizontal.then(|| {
                bar(
                    false,
                    size.width - corner(vertical),
                    size.height,
                    size.width,
                    offset.x,
                    limit.x,
                )
            }),
        ]
    }

    /// Position of a viewport-local `point` along this bar's strip.
    pub fn along(&self, point: Point) -> f32 {
        if self.vertical {
            point.y - self.strip.origin.y
        } else {
            point.x - self.strip.origin.x
        }
    }

    fn track(&self) -> f32 {
        if self.vertical {
            self.strip.size.height
        } else {
            self.strip.size.width
        }
    }

    /// Distance the thumb can move.
    pub fn travel(&self) -> f32 {
        self.track() - self.length
    }

    /// Where the thumb is drawn.
    pub fn thumb_rect(&self) -> Rect {
        self.span(self.thumb, self.length)
    }

    /// The whole range the thumb moves along.
    pub fn track_rect(&self) -> Rect {
        self.span(0.0, self.track())
    }

    /// Offset from the thumb's start at which a press at `along` grabs it: where
    /// it landed on the thumb, or the thumb's middle for a press elsewhere.
    pub fn grab(&self, along: f32) -> f32 {
        if (self.thumb..self.thumb + self.length).contains(&along) {
            along - self.thumb
        } else {
            self.length * 0.5
        }
    }

    /// Scroll fraction (0 at the start edge to 1) for a pointer at `along`
    /// holding the thumb at `grab`, or `None` when the thumb fills its track.
    pub fn fraction(&self, along: f32, grab: f32) -> Option<f32> {
        let travel = self.travel();
        (travel > 0.0).then(|| {
            let fraction = ((along - grab) / travel).clamp(0.0, 1.0);
            if self.rtl && !self.vertical {
                1.0 - fraction
            } else {
                fraction
            }
        })
    }

    fn span(&self, offset: f32, length: f32) -> Rect {
        let s = self.strip;
        if self.vertical {
            let width = THICKNESS.min(s.size.width);
            let x = if self.rtl {
                s.origin.x + MARGIN.min(s.size.width - width)
            } else {
                (s.origin.x + s.size.width - MARGIN - width).max(s.origin.x)
            };
            Rect::new(x, s.origin.y + offset, width, length)
        } else {
            let height = THICKNESS.min(s.size.height);
            let y = (s.origin.y + s.size.height - MARGIN - height).max(s.origin.y);
            Rect::new(s.origin.x + offset, y, length, height)
        }
    }
}

/// Paints each bar's track and thumb with rounded ends.
pub fn paint(
    builder: &mut SceneBuilder,
    bars: [Option<Bar>; 2],
    [track, thumb]: [Color; 2],
    radius: f32,
) -> Result<(), SceneError> {
    let radius = radius.min(THICKNESS * 0.5);
    for bar in bars.into_iter().flatten() {
        for (rect, color) in [(bar.track_rect(), track), (bar.thumb_rect(), thumb)] {
            if !rect.is_empty() {
                builder.fill(RoundedRect::new(rect, radius)?, color)?;
            }
        }
    }
    Ok(())
}
