//! Menu items: commands, check items, submenu openers and menu bar entries.

use std::{any::Any, cell::RefCell, rc::Rc};

use aegle_controls::{Input, Outcome};
use aegle_core::{Dirty, NodeId};
use aegle_layout::Dimension;
use aegle_scene::Affine;
use aegle_text::{Paragraph, TextSystem};
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

pub(crate) type Handlers = Rc<RefCell<Vec<Box<dyn FnMut(MenuItem) -> Result>>>>;

/// The control inside a [`MenuItem`] node, also used for menu bar entries.
pub struct MenuItemControl {
    pub(crate) button: aegle_controls::Button,
    pub(crate) text: Paragraph,
    /// Whether a check item is checked; `None` for other items.
    pub(crate) checked: Option<bool>,
    /// The submenu or, on a menu bar, the menu this item opens.
    pub(crate) submenu: Option<NodeId>,
    /// Whether that menu is shown.
    pub(crate) expanded: bool,
    pub(crate) bar: bool,
    pub(crate) handlers: Handlers,
}

impl Control for MenuItemControl {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn kind(&self) -> ControlKind {
        ControlKind::Button
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
    fn handle(&mut self, _: &mut InputCx<'_>, input: Input<'_>) -> Result<Outcome> {
        Ok(self.button.handle(input))
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
        if self.checked == Some(true) {
            let y = (size.height - MARK) * 0.5;
            check_mark(
                cx.builder,
                mirror(padding, MARK),
                y,
                MARK,
                cx.appearance.indicator,
            )?;
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
                cx.node.set_role(Role::MenuItemCheckBox);
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

handle!(
    MenuItem,
    "A menu entry: a command, a check item, or the opener of a submenu."
);

impl MenuItem {
    /// Replaces the item text.
    pub fn set_text(&self, text: &str) -> Result {
        self.change(|state, id| state.set_text(id, text))
    }
    /// Whether a check item is checked; always false for other items.
    pub fn is_checked(&self) -> Result<bool> {
        self.change(|state, id| Ok(item(state, id).checked == Some(true)))
    }
    /// Checks or unchecks a check item without calling the click handlers;
    /// an item created by [`crate::Menu::item`] becomes a check item.
    pub fn set_checked(&self, checked: bool) -> Result {
        self.change(|state, id| {
            item(state, id).checked = Some(checked);
            state.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
            Ok(())
        })
    }
    /// Activates the item as a click or Enter would.
    pub fn activate(&self) -> Result {
        self.change(|state, id| state.dispatch(id, Input::Activate))
    }
    /// Adds a handler run when the user chooses the item, after its menus
    /// closed and a check item toggled. Items that open a submenu do not run
    /// handlers. Handlers run in registration order outside UI borrows.
    pub fn on_click(&self, callback: impl FnMut(MenuItem) -> Result + 'static) -> Result {
        let handlers = self.change(|state, id| Ok(item(state, id).handlers.clone()))?;
        handlers.borrow_mut().push(Box::new(callback));
        Ok(())
    }
    /// Removes the click handlers.
    pub fn clear_on_click(&self) -> Result {
        let handlers = self.change(|state, id| Ok(item(state, id).handlers.clone()))?;
        handlers.borrow_mut().clear();
        Ok(())
    }
}

pub(crate) fn item(state: &mut State, id: NodeId) -> &mut MenuItemControl {
    state.control_as::<MenuItemControl>(id).unwrap()
}
