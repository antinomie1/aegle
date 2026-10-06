//! Named grid lines, `repeat()`, named areas and placement by name, with the
//! `grid` feature. These follow CSS Grid: an item placed by an area name
//! spans the lines `<name>-start` and `<name>-end` that the area defines.

use taffy::{
    GridPlacement, GridTemplateArea, GridTemplateAreas, GridTemplateComponent,
    GridTemplateRepetition, Line, RepetitionCount,
};

use crate::{Placement, Track};

/// How often a [`TemplateItem::Repeat`] repeats its tracks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repeat {
    /// Exactly this many times, at least once.
    Count(u16),
    /// As often as fits the grid's size, keeping empty repetitions (CSS `auto-fill`).
    AutoFill,
    /// As often as fits, collapsing empty repetitions (CSS `auto-fit`).
    AutoFit,
}

/// One entry of an explicit track list, as in CSS `grid-template-columns`.
#[derive(Clone, Debug, PartialEq)]
pub enum TemplateItem {
    /// Names the line at this position; several names may share a line.
    Line(String),
    /// One track.
    Track(Track),
    /// Tracks and line names repeated. Repeats do not nest; a list has at most
    /// one automatic repeat, and then every track in it must have a fixed
    /// size: pixels, a percentage or `MinMax` with a pixel minimum.
    Repeat(Repeat, Vec<TemplateItem>),
}

impl From<Track> for TemplateItem {
    fn from(track: Track) -> Self {
        Self::Track(track)
    }
}

/// Taffy's track components and line names for one axis.
pub type Template = (Vec<GridTemplateComponent<String>>, Vec<Vec<String>>);

fn fixed(track: Track) -> bool {
    matches!(track, Track::Px(_) | Track::Percent(_) | Track::MinMax(..))
}

/// Line names before each track of `items`, and after the last: `None` if a
/// nested repeat, an empty name or an invalid track is found.
fn lines(items: &[TemplateItem]) -> Option<(Vec<Track>, Vec<Vec<String>>)> {
    let (mut tracks, mut names) = (Vec::new(), vec![Vec::new()]);
    for item in items {
        match item {
            TemplateItem::Line(name) if !name.is_empty() => {
                names.last_mut().unwrap().push(name.clone())
            }
            TemplateItem::Track(track) if track.is_valid() => {
                tracks.push(*track);
                names.push(Vec::new());
            }
            _ => return None,
        }
    }
    Some((tracks, names))
}

/// Converts a track list, or `None` if it breaks a rule of [`TemplateItem`].
pub fn template(items: &[TemplateItem]) -> Option<Template> {
    let (mut components, mut names) = (Vec::new(), vec![Vec::new()]);
    let mut automatic = false;
    let mut all_fixed = true;
    for item in items {
        match item {
            TemplateItem::Repeat(count, inner) => {
                let (tracks, inner_names) = lines(inner)?;
                if tracks.is_empty() || *count == Repeat::Count(0) {
                    return None;
                }
                let count = match count {
                    Repeat::Count(n) => RepetitionCount::Count(*n),
                    Repeat::AutoFill | Repeat::AutoFit if automatic => return None,
                    Repeat::AutoFill => RepetitionCount::AutoFill,
                    Repeat::AutoFit => RepetitionCount::AutoFit,
                };
                automatic |= !matches!(count, RepetitionCount::Count(_));
                all_fixed &= tracks.iter().all(|&t| fixed(t));
                let named = inner_names.iter().any(|set| !set.is_empty());
                components.push(GridTemplateComponent::Repeat(GridTemplateRepetition {
                    count,
                    tracks: tracks.iter().map(|t| t.sizing()).collect(),
                    line_names: if named { inner_names } else { Vec::new() },
                }));
                names.push(Vec::new());
            }
            item => {
                let (tracks, mut item_names) = lines(std::slice::from_ref(item))?;
                names.last_mut().unwrap().append(&mut item_names[0]);
                if let Some(&track) = tracks.first() {
                    all_fixed &= fixed(track);
                    components.push(track.template());
                    names.push(Vec::new());
                }
            }
        }
    }
    if automatic && !all_fixed {
        return None;
    }
    let named = names.iter().any(|set| !set.is_empty())
        || components
            .iter()
            .any(|c| matches!(c, GridTemplateComponent::Repeat(r) if !r.line_names.is_empty()));
    // Taffy pairs each name set with the component after it, so repeat line
    // names are only found when the outer sets are present too.
    Some((components, if named { names } else { Vec::new() }))
}

/// Parses CSS `grid-template-areas` rows: whitespace-separated cell names, the
/// same count in every row, `.` for an unnamed cell; each name must cover a
/// rectangle. `None` for an empty, ragged or non-rectangular template.
pub fn areas<S: AsRef<str>>(rows: &[S]) -> Option<GridTemplateAreas<String>> {
    let mut found: Vec<(&str, [usize; 4], usize)> = Vec::new();
    let mut columns = None;
    for (row, text) in rows.iter().enumerate() {
        let mut count = 0;
        for (column, name) in text.as_ref().split_whitespace().enumerate() {
            count += 1;
            if name.chars().all(|c| c == '.') {
                continue;
            }
            match found.iter_mut().find(|(n, ..)| *n == name) {
                Some((_, [r0, c0, r1, c1], cells)) => {
                    (*r0, *c0) = ((*r0).min(row), (*c0).min(column));
                    (*r1, *c1) = ((*r1).max(row), (*c1).max(column));
                    *cells += 1;
                }
                None => found.push((name, [row, column, row, column], 1)),
            }
        }
        if count == 0 || columns.is_some_and(|c| c != count) {
            return None;
        }
        columns = Some(count);
    }
    let line = |index: usize| u16::try_from(index + 1).ok();
    let areas = found
        .into_iter()
        .map(|(name, [r0, c0, r1, c1], cells)| {
            ((r1 - r0 + 1) * (c1 - c0 + 1) == cells).then_some(())?;
            Some(GridTemplateArea {
                name: name.to_owned(),
                row_start: line(r0)?,
                row_end: line(r1 + 1)?,
                column_start: line(c0)?,
                column_end: line(c1 + 1)?,
            })
        })
        .collect::<Option<_>>()?;
    Some(GridTemplateAreas {
        areas,
        row_count: u16::try_from(rows.len()).ok()?,
        column_count: u16::try_from(columns?).ok()?,
    })
}

/// One edge of a grid item's placement on an axis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GridLine {
    /// Placed automatically.
    Auto,
    /// A line number, 1-based; negative counts from the end.
    Line(i16),
    /// Covering this many tracks from the other edge.
    Span(u16),
    /// The nth line with this name, or with `<name>-start`/`<name>-end` from
    /// an area; negative counts from the end.
    Named(String, i16),
    /// Extending from the other edge to the nth line with this name.
    NamedSpan(String, u16),
}

impl GridLine {
    fn is_valid(&self) -> bool {
        match self {
            Self::Auto => true,
            Self::Line(n) => *n != 0,
            Self::Span(n) => *n > 0,
            Self::Named(name, n) => !name.is_empty() && *n != 0,
            Self::NamedSpan(name, n) => !name.is_empty() && *n > 0,
        }
    }

    fn placement(&self) -> GridPlacement<String> {
        match self {
            Self::Auto => GridPlacement::Auto,
            Self::Line(n) => GridPlacement::Line((*n).into()),
            Self::Span(n) => GridPlacement::Span(*n),
            Self::Named(name, n) => GridPlacement::NamedLine(name.clone(), *n),
            Self::NamedSpan(name, n) => GridPlacement::NamedSpan(name.clone(), *n),
        }
    }
}

/// A grid item's start and end edges on one axis, as in CSS `grid-column`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridLines {
    /// The start edge.
    pub start: GridLine,
    /// The end edge.
    pub end: GridLine,
}

impl GridLines {
    /// Both edges at lines called `name`: the extent of an area with that
    /// name, or the line itself, like CSS `grid-column: name`.
    pub fn named(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            start: GridLine::Named(name.clone(), 1),
            end: GridLine::Named(name, 1),
        }
    }

    /// Whether lines are nonzero, spans positive and names not empty.
    pub fn is_valid(&self) -> bool {
        self.start.is_valid() && self.end.is_valid()
    }

    /// As Taffy's start/end placement.
    pub fn lines(&self) -> Line<GridPlacement<String>> {
        Line {
            start: self.start.placement(),
            end: self.end.placement(),
        }
    }
}

impl From<Placement> for GridLines {
    fn from(placement: Placement) -> Self {
        Self {
            start: placement.line.map_or(GridLine::Auto, GridLine::Line),
            end: GridLine::Span(placement.span),
        }
    }
}
