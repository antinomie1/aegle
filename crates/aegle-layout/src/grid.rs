//! Grid tracks and item placement, available with the `grid` feature.

use taffy::{
    GridAutoFlow, GridPlacement, GridTemplateComponent, Line, MaxTrackSizingFunction,
    MinTrackSizingFunction, TrackSizingFunction,
};

/// The size of one grid row or column.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Track {
    /// Fixed logical pixels.
    Px(f32),
    /// Percent (0–100) of the grid's content box on that axis.
    Percent(f32),
    /// A share of the free space, like QML's `Layout.fillWidth` with a stretch factor.
    Fr(f32),
    /// Sized to content, then stretched by free space.
    Auto,
    /// The narrowest content size.
    MinContent,
    /// The widest content size.
    MaxContent,
    /// Content-sized, but no larger than the given logical pixels.
    FitContent(f32),
    /// At least the first value in logical pixels, sharing free space by the
    /// second as a fraction: CSS `minmax(px, fr)`.
    MinMax(f32, f32),
}

impl Track {
    /// Whether sizes are finite and nonnegative, with a positive share for fractions.
    pub fn is_valid(self) -> bool {
        let size = |v: f32| v.is_finite() && v >= 0.0;
        match self {
            Self::Px(v) | Self::Percent(v) | Self::FitContent(v) => size(v),
            Self::Fr(v) => size(v),
            Self::MinMax(min, fr) => size(min) && size(fr),
            Self::Auto | Self::MinContent | Self::MaxContent => true,
        }
    }

    /// As an explicit or implicit Taffy track.
    pub fn sizing(self) -> TrackSizingFunction {
        let both = |min, max| TrackSizingFunction { min, max };
        match self {
            Self::Px(v) => both(
                MinTrackSizingFunction::length(v),
                MaxTrackSizingFunction::length(v),
            ),
            Self::Percent(v) => both(
                MinTrackSizingFunction::percent(v / 100.0),
                MaxTrackSizingFunction::percent(v / 100.0),
            ),
            // CSS resolves a lone `<flex>` track to `minmax(auto, <flex>)`.
            Self::Fr(v) => both(
                MinTrackSizingFunction::auto(),
                MaxTrackSizingFunction::fr(v),
            ),
            Self::Auto => both(
                MinTrackSizingFunction::auto(),
                MaxTrackSizingFunction::auto(),
            ),
            Self::MinContent => both(
                MinTrackSizingFunction::min_content(),
                MaxTrackSizingFunction::min_content(),
            ),
            Self::MaxContent => both(
                MinTrackSizingFunction::max_content(),
                MaxTrackSizingFunction::max_content(),
            ),
            Self::FitContent(v) => both(
                MinTrackSizingFunction::auto(),
                MaxTrackSizingFunction::fit_content_px(v),
            ),
            Self::MinMax(min, fr) => both(
                MinTrackSizingFunction::length(min),
                MaxTrackSizingFunction::fr(fr),
            ),
        }
    }

    /// As a `grid-template-*` component.
    pub fn template(self) -> GridTemplateComponent<String> {
        GridTemplateComponent::Single(self.sizing())
    }
}

/// Where an item sits on one grid axis: a start line (1-based; negative counts
/// from the end) or automatic placement, spanning one or more tracks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    /// The start line, or `None` for automatic placement.
    pub line: Option<i16>,
    /// Number of tracks covered, at least one.
    pub span: u16,
}

impl Placement {
    /// Automatic placement covering one track.
    pub const AUTO: Self = Self {
        line: None,
        span: 1,
    };

    /// Starts at `line` and covers one track.
    pub fn at(line: i16) -> Self {
        Self {
            line: Some(line),
            span: 1,
        }
    }

    /// Automatic placement covering `span` tracks.
    pub fn span(span: u16) -> Self {
        Self { line: None, span }
    }

    /// The same start, covering `span` tracks.
    pub fn spanning(self, span: u16) -> Self {
        Self { span, ..self }
    }

    /// Whether the start is not line 0, which CSS grids do not have, and the span is positive.
    pub fn is_valid(self) -> bool {
        self.line != Some(0) && self.span > 0
    }

    /// As Taffy's start/end placement.
    pub fn lines(self) -> Line<GridPlacement<String>> {
        Line {
            start: self
                .line
                .map_or(GridPlacement::Auto, |line| GridPlacement::Line(line.into())),
            end: GridPlacement::Span(self.span),
        }
    }
}

/// The order in which automatically placed items fill the grid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Flow {
    /// Fill each row, adding rows as needed.
    #[default]
    Row,
    /// Fill each column, adding columns as needed.
    Column,
    /// Like `Row`, backfilling earlier holes with later small items.
    RowDense,
    /// Like `Column`, backfilling earlier holes.
    ColumnDense,
}

impl Flow {
    /// As Taffy's auto-flow.
    pub fn auto_flow(self) -> GridAutoFlow {
        match self {
            Self::Row => GridAutoFlow::Row,
            Self::Column => GridAutoFlow::Column,
            Self::RowDense => GridAutoFlow::RowDense,
            Self::ColumnDense => GridAutoFlow::ColumnDense,
        }
    }
}
