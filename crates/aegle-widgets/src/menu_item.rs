//! Menu items: commands, check and radio items, submenu openers and menu
//! bar entries.

use aegle_controls::{Input, Outcome};
use aegle_core::{Dirty, NodeId};
use aegle_layout::Dimension;
use aegle_scene::{Affine, Rect, RoundedRect};
use aegle_text::{Paragraph, TextStyle, TextSystem};
use aegle_theme::{ControlKind, Theme};
use aegle_types::Size;
use aegle_ui::{
    Control, Result, State,
    control::{ControlVisual, InputCx, MeasureCx, PaintCx},
    handle,
};

use crate::paint::{CHEVRON, check_mark, chevron};

/// Width of the check column every menu item reserves.
const MARK: f32 = 12.0;

/// The control inside a [`MenuItem`] node, also used for menu bar entries.
pub struct MenuItemControl {
    pub(crate) button: aegle_controls::Button,
    pub(crate) text: Paragraph,
    /// Whether a check or radio item is checked; `None` for other items.
    pub(crate) checked: Option<bool>,
    /// Whether the item is exclusive among the radio items next to it.
    pub(crate) radio: bool,
    /// The shortcut hint shown at the end of the item.
    pub(crate) shortcut: Option<Paragraph>,
    /// The submenu or, on a menu bar, the menu this item opens.
    pub(crate) submenu: Option<NodeId>,
    /// Whether that menu is shown.
    pub(crate) expanded: bool,
    pub(crate) bar: bool,
}

impl Control for MenuItemControl {
    fn kind(&self) -> &'static ControlKind {
        &crate::kinds::MENU_ITEM
    }
    fn interactive(&self) -> bool {
        true
    }
    fn self_clipping(&self) -> bool {
        true
    }
    fn paragraph(&self) -> Option<&Paragraph> {
        Some(&self.text)
    }
    fn paragraph_mut(&mut self) -> Option<&mut Paragraph> {
        Some(&mut self.text)
    }
    fn restyle(&mut self, fonts: &mut TextSystem, style: &TextStyle<'_>) -> Result {
        fonts.restyle(&mut self.text, style)?;
        if let Some(shortcut) = &mut self.shortcut {
            fonts.restyle(shortcut, style)?;
        }
        Ok(())
    }
    fn visual(&self) -> ControlVisual {
        ControlVisual {
            pressed: self.button.is_pressed(),
            hovered: Some(self.button.is_hovered()),
            ..Default::default()
        }
    }
    fn set_enabled(&mut self, _: &mut TextSystem, enabled: bool) -> Outcome {
        self.button.set_enabled(enabled)
    }
    fn handle(&mut self, cx: &mut InputCx<'_>, input: Input<'_>) -> Result<Outcome> {
        let mut outcome = self.button.handle(input);
        if outcome.action.is_some() {
            cx.deferred.push(Box::new(crate::menu::chosen));
            // An item opening a menu has no handlers to run.
            if self.submenu.is_some() {
                outcome.action = None;
            }
        }
        Ok(outcome)
    }
    fn hover(
        &mut self,
        cx: &mut InputCx<'_>,
        _: aegle_ui::PointerId,
        input: Input<'_>,
    ) -> Result<Outcome> {
        self.handle(cx, input)
    }
    fn baseline(&self, size: Size, _: f32) -> Option<f32> {
        Some((size.height - self.text.size().height) / 2.0 + self.text.first_baseline()?)
    }
    fn measure(&mut self, cx: &MeasureCx<'_>) -> Result<Size> {
        let mut width = self.text.size().width + 2.0 * cx.padding;
        if !self.bar {
            width += MARK + cx.gap;
            if self.submenu.is_some() {
                width += cx.gap + CHEVRON;
            }
            if let Some(shortcut) = &self.shortcut {
                width += 2.0 * cx.gap + shortcut.size().width;
            }
        }
        Ok(Size::new(width, self.text.size().height + 2.0 * cx.padding))
    }
    fn retheme(
        &self,
        theme: &Theme,
        local: aegle_ui::LocalLayout,
        _: bool,
        style: &mut aegle_layout::Style,
    ) {
        if !local.contains(aegle_ui::LocalLayout::HEIGHT) {
            style.size.height = Dimension::length(theme.control_height);
        }
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) -> Result {
        let (size, padding, gap) = (cx.size, cx.padding, cx.theme.gap);
        let color = cx.appearance.foreground;
        cx.builder.push_clip(cx.shape)?;
        // Mirrored right to left: the check column and text start at the right.
        let mirror = |x: f32, width: f32| if cx.rtl { size.width - x - width } else { x };
        let text = self.text.size();
        let x = if self.bar {
            (size.width - text.width) / 2.0
        } else {
            mirror(padding + MARK + gap, text.width)
        };
        let y = (size.height - text.height) / 2.0;
        cx.builder.push_transform(Affine::translation(x, y)?)?;
        self.text.paint_with_color(cx.builder, color)?;
        cx.builder.pop()?;
        if let Some(shortcut) = &self.shortcut {
            let width = shortcut.size().width;
            let x = mirror(size.width - padding - width, width);
            let y = (size.height - shortcut.size().height) / 2.0;
            let color = match cx.visual.enabled {
                true => cx.theme.muted,
                false => color,
            };
            cx.builder.push_transform(Affine::translation(x, y)?)?;
            shortcut.paint_with_color(cx.builder, color)?;
            cx.builder.pop()?;
        }
        if self.checked == Some(true) {
            let (x, y) = (mirror(padding, MARK), (size.height - MARK) * 0.5);
            let indicator = cx.appearance.indicator;
            if self.radio {
                let dot = Rect::new(x + MARK * 0.25, y + MARK * 0.25, MARK * 0.5, MARK * 0.5);
                cx.builder
                    .fill(RoundedRect::new(dot, MARK * 0.25)?, indicator)?;
            } else {
                check_mark(cx.builder, x, y, MARK, indicator)?;
            }
        }
        if !self.bar && self.submenu.is_some() {
            // The dropdown chevron turned toward the side the submenu opens on.
            let x = mirror(size.width - padding - CHEVRON, CHEVRON) + CHEVRON * 0.5;
            let turn = if cx.rtl { 1.0 } else { -1.0 };
            let transform = Affine::new([0.0, turn, -turn, 0.0, x, size.height * 0.5])?;
            cx.builder.push_transform(transform)?;
            chevron(cx.builder, 0.0, 0.0, color)?;
            cx.builder.pop()?;
        }
        cx.builder.pop()?;
        Ok(())
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        use aegle_ui::accesskit::{Action, HasPopup, Role, Toggled};
        match self.checked {
            Some(checked) => {
                cx.node.set_role(match self.radio {
                    true => Role::MenuItemRadio,
                    false => Role::MenuItemCheckBox,
                });
                cx.node.set_toggled(if checked {
                    Toggled::True
                } else {
                    Toggled::False
                });
            }
            None => cx.node.set_role(Role::MenuItem),
        }
        if self.submenu.is_some() {
            cx.node.set_has_popup(HasPopup::Menu);
            cx.node.set_expanded(self.expanded);
        }
        if !cx.labelled {
            cx.node.set_label(self.text.text());
        }
        if let Some(shortcut) = &self.shortcut {
            cx.node.set_keyboard_shortcut(shortcut.text());
        }
        if cx.enabled {
            cx.node.add_action(Action::Focus);
            cx.node.add_action(Action::Click);
        }
    }
    #[cfg(feature = "accessibility")]
    fn action_input(
        &self,
        action: aegle_ui::accesskit::Action,
        _: Option<&aegle_ui::accesskit::ActionData>,
    ) -> Option<Input<'static>> {
        (action == aegle_ui::accesskit::Action::Click).then_some(Input::Activate)
    }
}

handle! {
    /// A menu entry: a command, a check or radio item, or the opener of a
    /// submenu.
    pub MenuItem(MenuItemControl)
}

impl MenuItem {
    /// Replaces the item text.
    pub fn set_text(&self, text: &str) -> Result {
        self.change(|state, id| state.set_text(id, text))
    }
    /// Shows `shortcut`, such as `"Ctrl+S"`, at the end of the item, or
    /// removes it with `None`. It is a hint, also the accessible keyboard
    /// shortcut; the application binds the keys itself.
    pub fn set_shortcut<'a>(&self, shortcut: impl Into<Option<&'a str>>) -> Result {
        let shortcut = shortcut.into();
        self.change(|state, id| {
            let shortcut = match shortcut {
                Some(text) => {
                    let style = state.text_style(id);
                    Some(state.fonts.borrow_mut().paragraph(text, &style)?)
                }
                None => None,
            };
            item(state, id).shortcut = shortcut;
            state.tree.mark_dirty(id, Dirty::ALL)?;
            Ok(())
        })
    }
    /// Whether a check or radio item is checked; always false for other items.
    pub fn is_checked(&self) -> Result<bool> {
        self.change(|state, id| Ok(item(state, id).checked == Some(true)))
    }
    /// Checks or unchecks a check or radio item without calling the click
    /// handlers; checking a radio item unchecks the rest of its group. An item
    /// created by [`crate::Menu::item`] becomes a check item.
    pub fn set_checked(&self, checked: bool) -> Result {
        self.change(|state, id| check(state, id, checked))
    }
    /// Activates the item as a click or Enter would.
    pub fn activate(&self) -> Result {
        self.change(|state, id| state.dispatch(id, Input::Activate))
    }
    /// Adds a handler run when the user chooses the item, after its menus
    /// closed and a check item toggled. Items that open a submenu do not run
    /// handlers. Handlers run in registration order outside UI borrows.
    pub fn on_click(&self, mut callback: impl FnMut(MenuItem) -> Result + 'static) -> Result {
        self.change(|state, id| state.on_action(id, move |node| callback(MenuItem(node))))
    }
}

pub(crate) fn item(state: &mut State, id: NodeId) -> &mut MenuItemControl {
    state.control_as::<MenuItemControl>(id).unwrap()
}

/// Checks or unchecks an item; checking a radio item unchecks the radio
/// items next to it, up to the nearest item or separator of another kind.
pub(crate) fn check(state: &mut State, id: NodeId, checked: bool) -> Result {
    let control = item(state, id);
    control.checked = Some(checked);
    let radio = control.radio;
    state.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
    if !(radio && checked) {
        return Ok(());
    }
    let parent = state.tree.parent(id)?.expect("menu items live in menus");
    let siblings: Vec<_> = state.tree.children(parent)?.collect();
    let at = siblings.iter().position(|&n| n == id).unwrap();
    let mut radio = |n: &&NodeId| {
        state
            .control_as::<MenuItemControl>(**n)
            .is_some_and(|c| c.radio)
    };
    let before = siblings[..at].iter().rev().take_while(&mut radio).count();
    let after = siblings[at + 1..].iter().take_while(&mut radio).count();
    for &sibling in siblings[at - before..=at + after]
        .iter()
        .filter(|&&n| n != id)
    {
        if item(state, sibling).checked.replace(false) == Some(true) {
            state
                .tree
                .mark_dirty(sibling, Dirty::PAINT | Dirty::SEMANTICS)?;
        }
    }
    Ok(())
}
