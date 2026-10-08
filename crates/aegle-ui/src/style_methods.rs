//! Typed-handle macros: the methods only some control kinds accept are added
//! to those kinds' handles, so the compiler rejects, say, a caret color on a
//! label instead of a runtime `WrongKind`.

/// Defines a typed handle: a clonable wrapper around a [`Node`](crate::Node)
/// that dereferences to it.
#[macro_export]
macro_rules! handle {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone)]
        pub struct $name(pub $crate::Node);
        impl ::std::ops::Deref for $name {
            type Target = $crate::Node;
            fn deref(&self) -> &$crate::Node {
                &self.0
            }
        }
    };
}

/// Defines one-field [`Style`](crate::Style) setters on a handle; each is a
/// shorthand for its field and ends a token binding of that field only.
#[doc(hidden)]
#[macro_export]
macro_rules! style_setters {
    ($($(#[$doc:meta])* $name:ident($value:ident: $ty:ty) => $field:ident, $slot:expr;)*) => {
        $($(#[$doc])*
        #[doc = concat!("\n\nShorthand for `Style::", stringify!($field), "`; ends a token binding of that field only.")]
        pub fn $name(&self, $value: $ty) -> $crate::Result {
            self.change(|state, id| {
                state.set_style_field(id, $slot.into(), |style| style.$field = Some($value))
            })
        })*
    };
}

/// Adds the style and typography methods only some control kinds accept to
/// typed handles. Groups: `text` (font size and face), `interactive` (hover
/// background and focus outline), `pressed` (pressed background), `indicator`
/// (marks and fills) and `editor` (selection and caret). The handle's control
/// kind must accept each listed group, see
/// [`StyleScope`](crate::control::StyleScope); [`Node`](crate::Node) itself
/// has the setters every kind accepts.
///
/// ```ignore
/// aegle_ui::handle!(Chip, "A toggleable chip.");
/// aegle_ui::style_methods!(Chip: text, interactive, pressed);
/// ```
#[macro_export]
macro_rules! style_methods {
    ($handle:ty: $($group:ident),+ $(,)?) => {
        $($crate::style_methods!(@$group $handle);)+
    };
    (@text $handle:ty) => {
        impl $handle {
            /// Sets a positive finite local text size, keeping text, selection
            /// and preedit; it does not inherit to children. Ends a font size
            /// token binding.
            pub fn set_font_size(&self, size: f32) -> $crate::Result {
                let slot = $crate::LengthSlot::FontSize.into();
                self.change(|state, id| {
                    state.write_unbound(id, slot, |state| state.set_font_size(id, Some(size)))
                })
            }
            /// Returns to the theme's font size, ending a font size token binding.
            pub fn clear_font_size(&self) -> $crate::Result {
                let slot = $crate::LengthSlot::FontSize.into();
                self.change(|state, id| {
                    state.write_unbound(id, slot, |state| state.set_font_size(id, None))
                })
            }
            /// Sets the font face, reshaping the text and keeping selection and
            /// preedit; it does not inherit to children. Ends a font token
            /// binding. Fails with InvalidValue for blank families or a weight
            /// outside 1–1000.
            pub fn set_font(&self, font: $crate::Font) -> $crate::Result {
                self.change(|state, id| {
                    state.write_unbound(id, $crate::TokenSlot::Font, |state| {
                        state.set_font(id, Some(font))
                    })
                })
            }
            /// Returns to `Font::DEFAULT`, ending a font token binding.
            pub fn clear_font(&self) -> $crate::Result {
                self.change(|state, id| {
                    state.write_unbound(id, $crate::TokenSlot::Font, |state| state.set_font(id, None))
                })
            }
            /// The local font face; `None` uses `Font::DEFAULT`.
            pub fn font(&self) -> $crate::Result<Option<$crate::Font>> {
                self.change(|state, id| Ok(state.decorations.get(&id).and_then(|d| d.font)))
            }
        }
    };
    (@interactive $handle:ty) => {
        impl $handle {
            $crate::style_setters! {
                /// Sets the background while enabled, hovered and not pressed.
                set_hover_background(color: $crate::Color) => hover_background, $crate::ColorSlot::HoverBackground;
                /// Sets the focus outline color.
                set_focus_color(color: $crate::Color) => focus_color, $crate::ColorSlot::FocusColor;
                /// Sets a nonnegative focus outline width; its target is zero
                /// while disabled or unfocused.
                set_focus_width(width: f32) => focus_width, $crate::LengthSlot::FocusWidth;
            }
        }
    };
    (@pressed $handle:ty) => {
        impl $handle {
            $crate::style_setters! {
                /// Sets the background while enabled and pressed.
                set_pressed_background(color: $crate::Color) => pressed_background, $crate::ColorSlot::PressedBackground;
            }
        }
    };
    (@indicator $handle:ty) => {
        impl $handle {
            $crate::style_setters! {
                /// Sets the color of check marks, switch knobs and slider or progress fills.
                set_indicator_color(color: $crate::Color) => indicator, $crate::ColorSlot::Indicator;
            }
        }
    };
    (@editor $handle:ty) => {
        impl $handle {
            $crate::style_setters! {
                /// Sets the selection fill, paired with the text foreground.
                set_selection_color(color: $crate::Color) => selection, $crate::ColorSlot::Selection;
                /// Sets the caret and preedit underline color.
                set_caret_color(color: $crate::Color) => caret, $crate::ColorSlot::Caret;
            }
        }
    };
}
