//! The default control library: every control `aegle` ships, built on the
//! `aegle-ui` engine through its [`Control`] trait and hooks.
//!
//! Content controls: [`Label`], [`Button`], [`TextField`] (single line and
//! multiline), [`CheckBox`], [`Switch`], [`Radio`], [`Slider`], [`Progress`],
//! [`ImageView`], [`Canvas`]. Containers and composites: [`ScrollView`],
//! [`ListView`] (virtual, equal or content-sized rows), [`Table`], [`Popup`],
//! [`Dropdown`], [`Menu`] and [`MenuBar`]. Create them through the [`Widgets`]
//! trait on `Container`, [`NodePopup::popup`], or [`NodeMenu`]. Each control owns its behavior (through `aegle-controls`),
//! default skin (the pure painters in this crate), layout defaults and semantics;
//! the engine owns the tree, input routing, focus, scrolling, motion and themes.
//! Virtual lists, popups and radio groups plug into the engine with [`HOOKS`].
//!
//! Kind-specific style setters (hover, pressed, indicator, caret, font) exist
//! only on the handles whose control accepts them. Features: `motion` forwards
//! to `aegle-ui/motion` and adds the slider and progress value glide;
//! `accessibility` forwards to `aegle-ui/accessibility` and adds the
//! controls' roles and actions. The `standalone` example drives these controls
//! without the `aegle` facade or a window; `custom_control` adds a control
//! with its own behavior through the same `Control` trait.

mod button;
mod canvas;
mod dropdown;
mod field;
mod group;
pub mod kinds;
mod label;
mod list;
mod menu;
mod menu_item;
mod menu_nav;
mod number_field;
mod numeric;
mod paint;
mod popup;
mod range_control;
mod scroll_view;
mod separator;
mod table;
mod tabs;
mod toggle;
mod tooltip;
mod visual;

use aegle_layout::Style;
use aegle_scene::{Image, SceneBuilder};
use aegle_theme::Theme;
use aegle_types::Size;
use aegle_ui::{Container, Control, Hooks, Node, Result, State};

pub use button::{Button, ButtonControl};
pub use canvas::{Canvas, CanvasControl, CanvasEvent, Painter};
pub use dropdown::Dropdown;
pub use field::{FieldControl, TextField};
pub use group::Group;
pub use label::{Label, LabelControl};
pub use list::ListView;
pub use menu::{Menu, MenuBar, NodeMenu};
pub use menu_item::{MenuItem, MenuItemControl};
pub use number_field::{NumberField, NumberFieldControl};
pub use numeric::{Progress, Slider};
pub use paint::{CHEVRON, Mark, ToggleSpec, check_mark, chevron, range, slider_track, toggle};
pub use popup::{NodePopup, Popup};
pub use range_control::{Orientation, ProgressControl, SliderControl};
pub use scroll_view::{ScrollControl, ScrollView};
pub use separator::{Separator, SeparatorControl, Splitter};
pub use table::{Table, TableColumn};
pub use tabs::{Tabs, TabsControl};
pub use toggle::{CheckBox, Radio, Switch, ToggleControl};
pub use tooltip::{NodeTooltip, TOOLTIP_DELAY};
pub use visual::{ImageControl, ImageView};

/// The engine hooks the controls need: popups (overlay placement, dismissal and
/// Escape/arrow keys), radio groups (arrow keys), and virtual lists (row
/// realization and measurement). Every constructor installs them on first use.
pub static HOOKS: Hooks = Hooks {
    key: Some(keys),
    press: Some(press),
    overlay_at: Some(popup::popup_at),
    place: Some(place),
    removed: Some(removed),
    removed_after: Some(pruned),
    measure: Some(list::measure_rows),
    realize: Some(list::realize_rows),
    hover: Some(hover),
    wake: Some(tooltip::wake),
};

fn keys(state: &mut State, key: &aegle_ui::KeyInput<'_>) -> Result<bool> {
    Ok(tooltip::tooltip_key(state, key)?
        || menu_nav::menu_key(state, key)?
        || popup::popup_key(state, key)?
        || toggle::radio_key(state, key)?
        || tabs::tab_key(state, key)?)
}

fn hover(state: &mut State, hit: Option<aegle_core::NodeId>) -> Result {
    tooltip::hovered(state, hit)?;
    menu_nav::hovered(state, hit)
}

fn press(state: &mut State, position: aegle_types::Point) -> Result {
    tooltip::hide(state)?;
    popup::dismiss_popups(state, position)
}

fn place(state: &mut State) -> bool {
    // Both run: each moves its own overlays.
    popup::place_popups(state) | tooltip::place(state)
}

fn removed(state: &mut State, node: aegle_core::NodeId) {
    popup::removed(state, node);
    list::removed(state, node);
    tooltip::removed(state, node);
}

fn pruned(state: &mut State) -> Result {
    popup::prune_popups(state)?;
    tooltip::prune(state)
}

/// Appends a node after installing [`HOOKS`].
pub(crate) fn add(
    container: &Container,
    create: impl FnOnce(&mut State, &Theme) -> Result<(Box<dyn Control>, Style)>,
) -> Result<Node> {
    container.add(|state, theme| {
        state.install(&HOOKS);
        create(state, theme)
    })
}

/// Creates the default controls inside a container.
pub trait Widgets {
    /// Appends a paragraph. Text wraps to available layout width.
    fn text(&self, text: &str) -> Result<Label>;
    /// Appends a neutral button with its visible text as the default accessible name.
    fn button(&self, text: &str) -> Result<Button>;
    /// Appends a single-line editor. Enter produces a submit action.
    fn text_field(&self, text: &str) -> Result<TextField>;
    /// Appends a wrapping multiline editor with a default four-line viewport.
    fn text_area(&self, text: &str) -> Result<TextField>;
    /// Appends a binary checkbox; the text is also its default accessible name.
    fn check_box(&self, text: &str, checked: bool) -> Result<CheckBox>;
    /// Appends a binary switch with a visible label.
    fn switch(&self, text: &str, checked: bool) -> Result<Switch>;
    /// Appends a radio button. Radio buttons sharing a parent form one group:
    /// checking one, by the user or [`Radio::set_checked`], unchecks the others.
    /// A newly created checked radio button unchecks its existing siblings.
    fn radio(&self, text: &str, checked: bool) -> Result<Radio>;
    /// Appends a continuous horizontal slider. Bounds must have a finite positive
    /// span; finite initial values clamp to them. Use set_step for discrete steps.
    fn slider(&self, min: f64, max: f64, value: f64) -> Result<Slider>;
    /// Appends a determinate progress bar with finite increasing bounds.
    fn progress(&self, min: f64, max: f64, value: f64) -> Result<Progress>;
    /// Appends an image whose intrinsic logical size is its pixel size. It keeps
    /// that size on the cross axis instead of stretching; `set_width` and
    /// `set_height` override it.
    fn image(&self, image: &Image) -> Result<ImageView>;
    /// Appends a canvas drawn by `painter` with zero intrinsic size; give it a
    /// size or flex grow. The painter runs during refresh while the UI is
    /// borrowed, so it must not use UI handles. Drawing is not clipped to the bounds.
    fn canvas(
        &self,
        painter: impl FnMut(&mut SceneBuilder, Size) -> Result + 'static,
    ) -> Result<Canvas>;
    /// Appends a scrollable column. Children retain their state outside the viewport.
    /// Both axes scroll on overflow; nested views pass unused wheel delta outward.
    fn scroll_view(&self) -> Result<ScrollView>;
    /// Appends a virtual list of `count` rows of `row_height` logical pixels.
    /// `row` fills an empty row column for an index. It runs during
    /// [`aegle_ui::Ui::refresh`] outside the UI borrow, so it may use any handle.
    /// `count × row_height` must not exceed 16,777,216.
    fn list_view(
        &self,
        row_height: f32,
        count: usize,
        row: impl FnMut(&Container, usize) -> Result + 'static,
    ) -> Result<ListView>;
    /// Appends a virtual list whose rows size to their content. Rows not yet
    /// shown count as `estimate` high; shown rows are measured after layout and
    /// later rows move accordingly. Scrolling back may shift content while
    /// estimates are replaced. `count × estimate` must not exceed 16,777,216.
    fn variable_list_view(
        &self,
        estimate: f32,
        count: usize,
        row: impl FnMut(&Container, usize) -> Result + 'static,
    ) -> Result<ListView>;
    /// Appends a table of `rows` rows of `row_height`, with at least one column.
    /// `fill(cell, row, column)` fills an empty cell column when its row becomes
    /// visible; it runs outside the UI borrow, like [`Widgets::list_view`] rows.
    /// Give the table a height or flex space.
    fn table(
        &self,
        columns: &[TableColumn],
        row_height: f32,
        rows: usize,
        fill: impl FnMut(&Container, usize, usize) -> Result + 'static,
    ) -> Result<Table>;
    /// Appends a dropdown with at least one choice and a valid selected index.
    /// The choice list opens below it; Up/Down and Enter or a click choose.
    fn dropdown(&self, items: &[&str], selected: usize) -> Result<Dropdown>;
    /// Appends a numeric field with finite increasing bounds; the value clamps.
    /// Typing commits on Enter or blur; steppers, arrows and the wheel step it.
    fn number_field(&self, min: f64, max: f64, value: f64) -> Result<NumberField>;
    /// Appends a one-pixel divider: vertical in a row, horizontal otherwise.
    fn separator(&self) -> Result<Separator>;
    /// Appends a tab list with one shown page per tab; see [`Tabs::add`].
    fn tabs(&self) -> Result<Tabs>;
    /// Appends a menu bar; add its menus with [`MenuBar::menu`].
    fn menu_bar(&self) -> Result<MenuBar>;
    /// Appends two panes divided by a draggable, keyboard-adjustable handle:
    /// side by side when horizontal, stacked when vertical. It grows to fill.
    fn splitter(&self, orientation: Orientation) -> Result<Splitter>;
}

impl Widgets for Container {
    fn text(&self, text: &str) -> Result<Label> {
        label::create(self, text)
    }
    fn button(&self, text: &str) -> Result<Button> {
        button::create(self, text)
    }
    fn text_field(&self, text: &str) -> Result<TextField> {
        field::create(self, text, false)
    }
    fn text_area(&self, text: &str) -> Result<TextField> {
        field::create(self, text, true)
    }
    fn check_box(&self, text: &str, checked: bool) -> Result<CheckBox> {
        toggle::check_box(self, text, checked)
    }
    fn switch(&self, text: &str, checked: bool) -> Result<Switch> {
        toggle::switch(self, text, checked)
    }
    fn radio(&self, text: &str, checked: bool) -> Result<Radio> {
        toggle::radio(self, text, checked)
    }
    fn slider(&self, min: f64, max: f64, value: f64) -> Result<Slider> {
        numeric::slider(self, min, max, value)
    }
    fn progress(&self, min: f64, max: f64, value: f64) -> Result<Progress> {
        numeric::progress(self, min, max, value)
    }
    fn image(&self, image: &Image) -> Result<ImageView> {
        visual::image(self, image)
    }
    fn canvas(
        &self,
        painter: impl FnMut(&mut SceneBuilder, Size) -> Result + 'static,
    ) -> Result<Canvas> {
        canvas::canvas(self, painter)
    }
    fn scroll_view(&self) -> Result<ScrollView> {
        scroll_view::create(self)
    }
    fn list_view(
        &self,
        row_height: f32,
        count: usize,
        row: impl FnMut(&Container, usize) -> Result + 'static,
    ) -> Result<ListView> {
        list::virtual_list(self, row_height, count, false, Box::new(row))
    }
    fn variable_list_view(
        &self,
        estimate: f32,
        count: usize,
        row: impl FnMut(&Container, usize) -> Result + 'static,
    ) -> Result<ListView> {
        list::virtual_list(self, estimate, count, true, Box::new(row))
    }
    fn table(
        &self,
        columns: &[TableColumn],
        row_height: f32,
        rows: usize,
        fill: impl FnMut(&Container, usize, usize) -> Result + 'static,
    ) -> Result<Table> {
        table::table(self, columns, row_height, rows, fill)
    }
    fn dropdown(&self, items: &[&str], selected: usize) -> Result<Dropdown> {
        dropdown::dropdown(self, items, selected)
    }
    fn number_field(&self, min: f64, max: f64, value: f64) -> Result<NumberField> {
        number_field::create(self, min, max, value)
    }
    fn separator(&self) -> Result<Separator> {
        separator::separator(self)
    }
    fn tabs(&self) -> Result<Tabs> {
        tabs::tabs(self)
    }
    fn menu_bar(&self) -> Result<MenuBar> {
        menu::menu_bar(self)
    }
    fn splitter(&self, orientation: Orientation) -> Result<Splitter> {
        separator::splitter(self, orientation)
    }
}
