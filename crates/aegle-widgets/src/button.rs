//! Buttons, and the dropdown and option variants the popup module builds on them.

use aegle_controls::{Input, Outcome};
use aegle_layout::{Dimension, Style};
use aegle_scene::Affine;
use aegle_text::{Paragraph, TextSystem};
use aegle_theme::{ControlKind, Theme};
use aegle_types::Size;
use aegle_ui::{
    Container, Control, Result,
    control::{ControlVisual, InputCx, MeasureCx, PaintCx},
    handle, text_style,
};

use crate::paint::{CHEVRON, check_mark, chevron};

handle! {
    /// A retained button with shared pointer, keyboard and semantic activation.
    pub Button(ButtonControl): text, interactive, pressed
}

impl Button {
    /// Replaces the button label.
    pub fn set_text(&self, text: &str) -> Result {
        self.change(|state, id| state.set_text(id, text))
    }
    /// Queues semantic activation using the normal enabled/visible behavior.
    pub fn activate(&self) -> Result {
        self.change(|state, id| state.dispatch(id, Input::Activate))
    }
    /// Adds a click handler; handlers run in registration order after the
    /// input batch, outside every UI/tree borrow, so they may safely create or
    /// remove controls.
    pub fn on_click(&self, mut callback: impl FnMut(Button) -> Result + 'static) -> Result {
        self.change(|state, id| state.on_action(id, move |node| callback(Button(node))))
    }
}

/// How a button reads: plain, the face of a dropdown, or a choice inside one.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum Variant {
    #[default]
    Plain,
    /// A dropdown's button; `expanded` is whether its choice list is shown.
    Dropdown { expanded: bool },
    /// A choice; `chosen` is whether it is the dropdown's selection.
    Option { chosen: bool },
    /// A tab in a tab list; `selected` is whether its page is shown.
    Tab { selected: bool },
}

/// The control inside a [`Button`] node.
pub struct ButtonControl {
    button: aegle_controls::Button,
    text: Paragraph,
    pub(crate) variant: Variant,
}

impl ButtonControl {
    pub(crate) fn new(
        fonts: &std::cell::RefCell<TextSystem>,
        text: &str,
        theme: &Theme,
    ) -> Result<Self> {
        Ok(Self {
            button: aegle_controls::Button::new(),
            text: fonts.borrow_mut().paragraph(text, &text_style(theme))?,
            variant: Variant::Plain,
        })
    }

    fn listed(&self) -> bool {
        matches!(
            self.variant,
            Variant::Dropdown { .. } | Variant::Option { .. }
        )
    }
}

impl Control for ButtonControl {
    fn kind(&self) -> &'static ControlKind {
        match self.variant {
            Variant::Plain | Variant::Dropdown { .. } => &crate::kinds::BUTTON,
            Variant::Option { .. } => &crate::kinds::MENU_ITEM,
            Variant::Tab { .. } => &crate::kinds::TAB,
        }
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
    fn handle(&mut self, cx: &mut InputCx<'_>, input: Input<'_>) -> Result<Outcome> {
        let mut outcome = self.button.handle(input);
        if outcome.action.is_some() {
            // The behavior of composite controls built on buttons.
            match self.variant {
                Variant::Plain => {}
                Variant::Dropdown { .. } => {
                    // A dropdown's handlers hear only choices.
                    outcome.action = None;
                    cx.deferred.push(Box::new(crate::dropdown::toggle));
                }
                Variant::Option { .. } => cx.deferred.push(Box::new(crate::dropdown::chosen)),
                Variant::Tab { .. } => cx.deferred.push(Box::new(crate::tabs::chosen)),
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
        // Painted centered vertically.
        Some((size.height - self.text.size().height) / 2.0 + self.text.first_baseline()?)
    }
    fn measure(&mut self, cx: &MeasureCx<'_>) -> Result<Size> {
        let chevron = if matches!(self.variant, Variant::Dropdown { .. }) {
            cx.gap + CHEVRON
        } else {
            0.0
        };
        Ok(Size::new(
            self.text.size().width + 2.0 * cx.padding + chevron,
            self.text.size().height + 2.0 * cx.padding,
        ))
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
        let (size, padding) = (cx.size, cx.padding);
        cx.builder.push_clip(cx.shape)?;
        // Dropdowns and their choices read as lists: text starts at the padding,
        // on the right right to left, with marks at the other end.
        let x = if self.listed() && cx.rtl {
            size.width - padding - self.text.size().width
        } else if self.listed() {
            padding
        } else {
            (size.width - self.text.size().width) / 2.0
        };
        cx.builder.push_transform(Affine::translation(
            x,
            (size.height - self.text.size().height) / 2.0,
        )?)?;
        self.text
            .paint_with_color(cx.builder, cx.appearance.foreground)?;
        cx.builder.pop()?;
        if matches!(self.variant, Variant::Dropdown { .. }) {
            let x = if cx.rtl {
                padding + CHEVRON * 0.5
            } else {
                size.width - padding - CHEVRON * 0.5
            };
            chevron(cx.builder, x, size.height * 0.5, cx.appearance.foreground)?;
        }
        if matches!(self.variant, Variant::Tab { selected: true }) {
            // The selected tab is marked by an underline, not only by color.
            let line =
                aegle_scene::Rect::new(padding * 0.5, size.height - 2.0, size.width - padding, 2.0);
            if !line.is_empty() {
                cx.builder.fill(
                    aegle_scene::RoundedRect::new(line, 0.0)?,
                    cx.appearance.indicator,
                )?;
            }
        }
        if matches!(self.variant, Variant::Option { chosen: true }) {
            // The current choice is marked by shape, not only color.
            let mark = 12.0_f32.min(size.height);
            let x = if cx.rtl {
                padding
            } else {
                size.width - padding - mark
            };
            let y = (size.height - mark) * 0.5;
            check_mark(cx.builder, x, y, mark, cx.appearance.indicator)?;
        }
        cx.builder.pop()?;
        Ok(())
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        use aegle_ui::accesskit::{Action, Role};
        cx.node.set_role(match self.variant {
            Variant::Plain => Role::Button,
            Variant::Dropdown { .. } => Role::ComboBox,
            Variant::Option { .. } => Role::ListBoxOption,
            Variant::Tab { .. } => Role::Tab,
        });
        match self.variant {
            Variant::Dropdown { expanded } => cx.node.set_expanded(expanded),
            Variant::Option { chosen } => cx.node.set_selected(chosen),
            Variant::Tab { selected } => cx.node.set_selected(selected),
            Variant::Plain => {}
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

pub(crate) fn create(container: &Container, text: &str) -> Result<Button> {
    create_as(container, text, Variant::Plain)
}

/// Appends a button reading as `variant`, which fixes its kind.
pub(crate) fn create_as(container: &Container, text: &str, variant: Variant) -> Result<Button> {
    crate::add(container, |state, theme| {
        let mut control = ButtonControl::new(&state.fonts, text, theme)?;
        control.variant = variant;
        Ok((
            Box::new(control),
            Style {
                size: aegle_layout::Size {
                    width: Dimension::auto(),
                    height: Dimension::length(theme.control_height),
                },
                flex_shrink: 0.0,
                ..Default::default()
            },
        ))
    })
    .map(Button)
}
