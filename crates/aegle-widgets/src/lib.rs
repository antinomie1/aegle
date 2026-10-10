//! The default control library: every control `aegle` ships, built on the
//! `aegle-ui` engine through its [`Control`] trait and hooks.
//!
//! Content controls: [`Label`], [`Button`], [`TextField`] (single line and
//! multiline), [`CheckBox`], [`Switch`], [`Radio`], [`Slider`], [`Progress`],
//! [`ImageView`], [`Canvas`]. Containers and composites: scroll views
//! (a `Container` whose children scroll), [`ListView`] (virtual, equal or
//! content-sized rows), [`Table`], [`Popup`] (also menus), [`Dropdown`] and
//! [`MenuBar`]. Composite handles dereference to their `Node`, not to a
//! container, so their own parts are the only children. Create them through the [`Widgets`]
//! trait on `Container`; [`NodeWidgets`] adds popups, menus and tooltips to
//! any control. Each control owns its behavior (through `aegle-controls`),
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
use aegle_ui::{Container, Control, HandlerResult, Hooks, Node, Result, State};

pub use button::{Button, ButtonControl};
pub use canvas::{Canvas, CanvasControl, CanvasEvent, Painter};
pub use dropdown::Dropdown;
pub use field::{FieldControl, TextField};
pub use group::Group;
pub use label::{Label, LabelControl};
pub use list::{ListView, RowHeight};
pub use menu::MenuBar;
pub use menu_item::{MenuItem, MenuItemControl};
pub use number_field::{NumberField, NumberFieldControl};
pub use numeric::{Progress, Slider};
pub use paint::{CHEVRON, Mark, ToggleSpec, check_mark, chevron, range, slider_track, toggle};
pub use popup::Popup;
pub use range_control::{Orientation, ProgressControl, SliderControl};
pub use scroll_view::ScrollControl;
pub use separator::{Separator, SeparatorControl, Splitter};
pub use table::{Table, TableColumn};
pub use tabs::{Tabs, TabsControl};
pub use toggle::{CheckBox, Radio, Switch, ToggleControl};
pub use tooltip::TOOLTIP_DELAY;
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
) -> Node {
    container.add(|state, theme| {
        state.install(&HOOKS);
        create(state, theme)
    })
}

/// Creates the default controls inside a container.
pub trait Widgets {
    /// Appends a paragraph. Text wraps to available layout width.
    fn text(&self, text: &str) -> Label;
    /// Appends a neutral button with its visible text as the default accessible name.
    fn button(&self, text: &str) -> Button;
    /// Appends a single-line editor. Enter produces a submit action.
    fn text_field(&self, text: &str) -> TextField;
    /// Appends a wrapping multiline editor with a default four-line viewport.
    fn text_area(&self, text: &str) -> TextField;
    /// Appends a binary checkbox; the text is also its default accessible name.
    fn check_box(&self, text: &str, checked: bool) -> CheckBox;
    /// Appends a binary switch with a visible label.
    fn switch(&self, text: &str, checked: bool) -> Switch;
    /// Appends a radio button. Radio buttons sharing a parent form one group:
    /// checking one, by the user or [`Radio::set_checked`], unchecks the others.
    /// A newly created checked radio button unchecks its existing siblings.
    fn radio(&self, text: &str, checked: bool) -> Radio;
    /// Appends a continuous horizontal slider. Bounds must have a finite positive
    /// span; finite initial values clamp to them. Use set_step for discrete steps.
    fn slider(&self, min: f64, max: f64, value: f64) -> Slider;
    /// Appends a determinate progress bar with finite increasing bounds.
    fn progress(&self, min: f64, max: f64, value: f64) -> Progress;
    /// Appends an image whose intrinsic logical size is its pixel size. It keeps
    /// that size on the cross axis instead of stretching; `set_width` and
    /// `set_height` override it.
    fn image(&self, image: &Image) -> ImageView;
    /// Appends a canvas drawn by `painter` with zero intrinsic size; give it a
    /// size or flex grow. The painter runs during refresh while the UI is
    /// borrowed, so it must not use UI handles. Drawing is not clipped to the bounds.
    fn canvas(&self, painter: impl FnMut(&mut SceneBuilder, Size) + 'static) -> Canvas;
    /// Appends a scrollable column. Children retain their state outside the viewport.
    /// Both axes scroll on overflow; nested views pass unused wheel delta outward.
    /// [`Node::scroll_to`] and the other scroll methods move it.
    fn scroll_view(&self) -> Container;
    /// Appends a virtual list of `count` rows: [`RowHeight::Fixed`] (or a plain
    /// `f32`) rows of one height, or [`RowHeight::Estimate`] rows sized to
    /// their content. `row` fills an empty row column for an index. It runs
    /// during [`aegle_ui::Ui::refresh`] outside the UI borrow, so it may use
    /// any handle. `count × height` must not exceed 16,777,216.
    fn list_view<R: HandlerResult>(
        &self,
        height: impl Into<RowHeight>,
        count: usize,
        row: impl FnMut(&Container, usize) -> R + 'static,
    ) -> ListView;
    /// Appends a table of `rows` rows of `row_height`, with at least one column.
    /// `fill(cell, row, column)` fills an empty cell column when its row becomes
    /// visible; it runs outside the UI borrow, like [`Widgets::list_view`] rows.
    /// Give the table a height or flex space.
    fn table<R: HandlerResult>(
        &self,
        columns: &[TableColumn],
        row_height: f32,
        rows: usize,
        fill: impl FnMut(&Container, usize, usize) -> R + 'static,
    ) -> Table;
    /// Appends a dropdown with at least one choice and a valid selected index.
    /// The choice list opens below it; Up/Down and Enter or a click choose.
    fn dropdown(&self, items: &[&str], selected: usize) -> Dropdown;
    /// Appends a numeric field with finite increasing bounds; the value clamps.
    /// Typing commits on Enter or blur; steppers, arrows and the wheel step it.
    fn number_field(&self, min: f64, max: f64, value: f64) -> NumberField;
    /// Appends a one-pixel divider: vertical in a row, horizontal otherwise.
    fn separator(&self) -> Separator;
    /// Appends a tab list with one shown page per tab; see [`Tabs::add`].
    fn tabs(&self) -> Tabs;
    /// Appends a menu bar; add its menus with [`MenuBar::menu`].
    fn menu_bar(&self) -> MenuBar;
    /// Appends two panes divided by a draggable, keyboard-adjustable handle:
    /// side by side when horizontal, stacked when vertical. It grows to fill.
    fn splitter(&self, orientation: Orientation) -> Splitter;
}

impl Widgets for Container {
    fn text(&self, text: &str) -> Label {
        label::create(self, text)
    }
    fn button(&self, text: &str) -> Button {
        button::create(self, text)
    }
    fn text_field(&self, text: &str) -> TextField {
        field::create(self, text, false)
    }
    fn text_area(&self, text: &str) -> TextField {
        field::create(self, text, true)
    }
    fn check_box(&self, text: &str, checked: bool) -> CheckBox {
        toggle::check_box(self, text, checked)
    }
    fn switch(&self, text: &str, checked: bool) -> Switch {
        toggle::switch(self, text, checked)
    }
    fn radio(&self, text: &str, checked: bool) -> Radio {
        toggle::radio(self, text, checked)
    }
    fn slider(&self, min: f64, max: f64, value: f64) -> Slider {
        numeric::slider(self, min, max, value)
    }
    fn progress(&self, min: f64, max: f64, value: f64) -> Progress {
        numeric::progress(self, min, max, value)
    }
    fn image(&self, image: &Image) -> ImageView {
        visual::image(self, image)
    }
    fn canvas(&self, painter: impl FnMut(&mut SceneBuilder, Size) + 'static) -> Canvas {
        canvas::canvas(self, painter)
    }
    fn scroll_view(&self) -> Container {
        scroll_view::create(self)
    }
    fn list_view<R: HandlerResult>(
        &self,
        height: impl Into<RowHeight>,
        count: usize,
        mut row: impl FnMut(&Container, usize) -> R + 'static,
    ) -> ListView {
        let row = Box::new(move |c: &Container, i| row(c, i).into_result());
        list::virtual_list(self, height.into(), count, row)
    }
    fn table<R: HandlerResult>(
        &self,
        columns: &[TableColumn],
        row_height: f32,
        rows: usize,
        mut fill: impl FnMut(&Container, usize, usize) -> R + 'static,
    ) -> Table {
        table::table(self, columns, row_height, rows, move |c, r, k| {
            fill(c, r, k).into_result()
        })
    }
    fn dropdown(&self, items: &[&str], selected: usize) -> Dropdown {
        dropdown::dropdown(self, items, selected)
    }
    fn number_field(&self, min: f64, max: f64, value: f64) -> NumberField {
        number_field::create(self, min, max, value)
    }
    fn separator(&self) -> Separator {
        separator::separator(self)
    }
    fn tabs(&self) -> Tabs {
        tabs::tabs(self)
    }
    fn menu_bar(&self) -> MenuBar {
        menu::menu_bar(self)
    }
    fn splitter(&self, orientation: Orientation) -> Splitter {
        separator::splitter(self, orientation)
    }
}

/// The default controls' additions to every control, like [`Widgets`] for
/// containers.
pub trait NodeWidgets {
    /// Creates a hidden, empty popup anchored to this control. Add content
    /// through the popup's container methods, then [`Popup::show`] it. It uses
    /// the anchor's theme and the theme surface with a border.
    fn popup(&self) -> Popup;
    /// Creates a hidden menu below this control: a popup filled through
    /// [`Popup::item`] and the other item methods, shown by [`Popup::show`],
    /// typically from a button's click handler.
    fn menu(&self) -> Popup;
    /// Creates a hidden menu shown where a context menu is requested over
    /// this control: at a secondary press, or at the focused control for
    /// the Menu key and Shift+F10 (see [`Node::on_context_menu`]).
    fn context_menu(&self) -> Popup;
    /// Shows `text` after the pointer rests on this control (or a descendant
    /// without its own tooltip) and sets it as the accessible description;
    /// `None` removes it. Pressing, Escape or leaving hides it.
    fn set_tooltip<'a>(&self, text: impl Into<Option<&'a str>>);
}

impl NodeWidgets for Node {
    fn popup(&self) -> Popup {
        popup::popup(self)
    }
    fn menu(&self) -> Popup {
        menu::menu(self)
    }
    fn context_menu(&self) -> Popup {
        menu::context_menu(self)
    }
    fn set_tooltip<'a>(&self, text: impl Into<Option<&'a str>>) {
        tooltip::set_tooltip(self, text.into())
    }
}
