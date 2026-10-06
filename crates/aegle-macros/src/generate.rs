use aegle_markup::{
    CheckedDocument, CheckedNode, CheckedProperty, Kind, PropertyName, Value as Literal,
};
use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};

/// Only validated nodes reach generation; each property has its schema's type.
pub(super) fn builder(document: &CheckedDocument, facade: &TokenStream) -> TokenStream {
    let root = &document.root;
    let root_handle = format_ident!("__aegle_node_0", span = Span::mixed_site());
    let parent = format_ident!("__aegle_parent", span = Span::mixed_site());
    let view = format_ident!("__AegleView", span = Span::mixed_site());
    let root_type = handle_type(root.kind, facade);
    let parent_type = if root.kind == Kind::Window {
        quote! { #facade::App }
    } else {
        quote! { #facade::Container }
    };
    let create_root = constructor(root, &quote! { #parent }, facade);
    let cleanup = if root.kind == Kind::Window {
        quote! { #root_handle.close()?; }
    } else {
        quote! { #root_handle.remove()?; }
    };
    let mut output = Output::default();
    output.visit(root, &root_handle, facade);
    let Output {
        creations,
        setters,
        transitions,
        fields,
        values,
        ..
    } = output;
    quote! {{
        #[allow(dead_code, non_snake_case)]
        struct #view {
            pub root: #root_type,
            #(#fields)*
        }
        move |#parent: &#parent_type| -> #facade::Result<#view> {
            use #facade::{NodeTooltip as _, Widgets as _};
            let #root_handle = #create_root?;
            let __aegle_result = (|| -> #facade::Result<#view> {
                #(#creations)*
                #(#setters)*
                #(#transitions)*
                ::core::result::Result::Ok(#view {
                    root: #root_handle.clone(),
                    #(#values)*
                })
            })();
            match __aegle_result {
                ::core::result::Result::Ok(view) => ::core::result::Result::Ok(view),
                ::core::result::Result::Err(error) => {
                    #cleanup
                    ::core::result::Result::Err(error)
                }
            }
        }
    }}
}

#[derive(Default)]
struct Output {
    count: usize,
    creations: Vec<TokenStream>,
    setters: Vec<TokenStream>,
    transitions: Vec<TokenStream>,
    fields: Vec<TokenStream>,
    values: Vec<TokenStream>,
}

impl Output {
    fn visit(&mut self, node: &CheckedNode, handle: &Ident, facade: &TokenStream) {
        if let Some(id) = &node.id {
            let field = Ident::new(id, Span::call_site());
            let ty = handle_type(node.kind, facade);
            self.fields.push(quote! { pub #field: #ty, });
            self.values.push(if self.count == 0 {
                quote! { #field: #handle.clone(), }
            } else {
                quote! { #field: #handle, }
            });
        }
        for property in &node.properties {
            if let Some(setter) = setter(node.kind, property, handle, facade) {
                self.setters.push(setter);
            }
        }
        if let Some(transition) = transition(node, handle, facade) {
            self.transitions.push(transition);
        }
        // A splitter's two children fill its panes.
        let panes = [quote! { #handle.first() }, quote! { #handle.second() }];
        for (index, child) in node.children.iter().enumerate() {
            self.count += 1;
            let child_handle =
                format_ident!("__aegle_node_{}", self.count, span = Span::mixed_site());
            let parent = if node.kind == Kind::Splitter {
                panes[index].clone()
            } else {
                quote! { #handle }
            };
            let constructor = constructor(child, &parent, facade);
            self.creations
                .push(quote! { let #child_handle = #constructor?; });
            self.visit(child, &child_handle, facade);
        }
    }
}

pub(super) fn handle_type(kind: Kind, facade: &TokenStream) -> TokenStream {
    let name = match kind {
        Kind::Window => "Window",
        Kind::Row | Kind::Column | Kind::Grid | Kind::Stack | Kind::Tab => "Container",
        Kind::Separator => "Separator",
        Kind::NumberField => "NumberField",
        Kind::Tabs => "Tabs",
        Kind::Splitter => "Splitter",
        Kind::ScrollView => "ScrollView",
        Kind::Text => "Label",
        Kind::Button => "Button",
        Kind::TextField | Kind::TextArea => "TextField",
        Kind::CheckBox => "CheckBox",
        Kind::RadioButton => "Radio",
        Kind::Switch => "Switch",
        Kind::Slider => "Slider",
        Kind::Progress => "Progress",
    };
    let name = Ident::new(name, Span::call_site());
    quote! { #facade::#name }
}

fn constructor(node: &CheckedNode, parent: &TokenStream, facade: &TokenStream) -> TokenStream {
    let text = node.properties.iter().find_map(|property| {
        if matches!(property.name, PropertyName::Title | PropertyName::Text) {
            let Literal::String(value) = &property.value else {
                unreachable!()
            };
            Some(value.as_str())
        } else {
            None
        }
    });
    if node.kind == Kind::Window {
        let title = text.unwrap_or("Aegle");
        let dimensions = node.properties.iter().filter_map(|property| {
            let field = match property.name {
                PropertyName::Width => quote! { width },
                PropertyName::Height => quote! { height },
                _ => return None,
            };
            let Literal::Length(value) = property.value else {
                unreachable!()
            };
            let value = value as u32;
            Some(quote! { #field: #value, })
        });
        return quote! {
            #parent.window_with_options(#title, #facade::WindowOptions {
                #(#dimensions)*
                ..::core::default::Default::default()
            })
        };
    }
    let text = text.unwrap_or("");
    if matches!(node.kind, Kind::CheckBox | Kind::Switch | Kind::RadioButton) {
        let checked = node
            .properties
            .iter()
            .find_map(|property| {
                if property.name == PropertyName::Checked {
                    let Literal::Bool(value) = property.value else {
                        unreachable!()
                    };
                    Some(value)
                } else {
                    None
                }
            })
            .unwrap_or(false);
        return match node.kind {
            Kind::CheckBox => quote! { #parent.check_box(#text, #checked) },
            Kind::Switch => quote! { #parent.switch(#text, #checked) },
            _ => quote! { #parent.radio(#text, #checked) },
        };
    }
    if matches!(node.kind, Kind::Slider | Kind::Progress | Kind::NumberField) {
        let mut range = [0.0_f64, 1.0, 0.0];
        for property in &node.properties {
            let index = match property.name {
                PropertyName::Min => 0,
                PropertyName::Max => 1,
                PropertyName::Value => 2,
                _ => continue,
            };
            let Literal::Number(value) = property.value else {
                unreachable!()
            };
            range[index] = f64::from(value);
        }
        let [min, max, value] = range;
        return match node.kind {
            Kind::Slider => quote! { #parent.slider(#min, #max, #value) },
            Kind::Progress => quote! { #parent.progress(#min, #max, #value) },
            _ => quote! { #parent.number_field(#min, #max, #value) },
        };
    }
    match node.kind {
        Kind::Row => quote! { #parent.row() },
        Kind::Column => quote! { #parent.column() },
        Kind::Grid => quote! { #parent.grid(&[]) },
        Kind::Stack => quote! { #parent.stack() },
        Kind::ScrollView => quote! { #parent.scroll_view() },
        Kind::Text => quote! { #parent.text(#text) },
        Kind::Button => quote! { #parent.button(#text) },
        Kind::TextField => quote! { #parent.text_field(#text) },
        Kind::TextArea => quote! { #parent.text_area(#text) },
        Kind::Separator => quote! { #parent.separator() },
        Kind::Tabs => quote! { #parent.tabs() },
        Kind::Tab => quote! { #parent.add(#text) },
        Kind::Splitter => {
            let orientation = orientation(node).unwrap_or("Horizontal");
            let orientation = Ident::new(orientation, Span::call_site());
            quote! { #parent.splitter(#facade::Orientation::#orientation) }
        }
        Kind::Window
        | Kind::NumberField
        | Kind::CheckBox
        | Kind::Switch
        | Kind::RadioButton
        | Kind::Slider
        | Kind::Progress => {
            unreachable!()
        }
    }
}

/// The Rust variant of an orientation identifier.
fn variant(value: &str) -> &'static str {
    if value == "vertical" {
        "Vertical"
    } else {
        "Horizontal"
    }
}

/// A node's literal orientation variant.
fn orientation(node: &CheckedNode) -> Option<&'static str> {
    node.properties
        .iter()
        .find_map(|property| match &property.value {
            Literal::Identifier(value) if property.name == PropertyName::Orientation => {
                Some(variant(value))
            }
            _ => None,
        })
}

fn setter(
    kind: Kind,
    property: &CheckedProperty,
    handle: &Ident,
    facade: &TokenStream,
) -> Option<TokenStream> {
    use PropertyName::*;
    if kind == Kind::Window && matches!(property.name, Width | Height)
        || kind == Kind::Splitter && property.name == Orientation
    {
        return None;
    }
    if let Some(call) = crate::layout::setter(property, handle, facade) {
        return Some(call);
    }
    let name = match property.name {
        Title | Text | Checked | Min | Max | Value | Transition | Easing => return None,
        Grow => "set_grow",
        Visible => "set_visible",
        Enabled => "set_enabled",
        Label => "set_accessible_label",
        ReadOnly => "set_read_only",
        Password => "set_password",
        Theme => "set_theme",
        Background => "set_background",
        Foreground => "set_foreground",
        BorderColor => "set_border_color",
        BorderWidth => "set_border_width",
        Radius => "set_radius",
        FocusColor => "set_focus_color",
        FocusWidth => "set_focus_width",
        SelectionColor => "set_selection_color",
        CaretColor => "set_caret_color",
        HoverBackground => "set_hover_background",
        PressedBackground => "set_pressed_background",
        DisabledBackground => "set_disabled_background",
        DisabledForeground => "set_disabled_foreground",
        FontSize => "set_font_size",
        Step => "set_step",
        Mixed => "set_mixed",
        IndicatorColor => "set_indicator_color",
        Indeterminate => "set_indeterminate",
        Decimals => "set_decimals",
        Ratio => "set_ratio",
        Orientation => "set_orientation",
        Tooltip => "set_tooltip",
        _ => unreachable!("layout properties are generated by crate::layout"),
    };
    let argument = match &property.value {
        Literal::String(value) if property.name == Tooltip => quote! { Some(#value) },
        Literal::String(value) => quote! { #value },
        Literal::Bool(value) => quote! { #value },
        Literal::Color([red, green, blue, alpha]) => {
            quote! { #facade::Color::rgba(#red, #green, #blue, #alpha) }
        }
        Literal::Number(value) | Literal::Length(value) => match property.name {
            Step => {
                let value = f64::from(*value);
                quote! { #value }
            }
            Decimals => {
                let value = *value as u8;
                quote! { #value }
            }
            _ => quote! { #value },
        },
        Literal::Percent(value) if property.name == Ratio => {
            let value = value / 100.0;
            quote! { #value }
        }
        Literal::Identifier(value) if property.name == Orientation => {
            let name = Ident::new(variant(value), Span::call_site());
            quote! { #facade::Orientation::#name }
        }
        Literal::Identifier(value) if property.name == Theme => {
            let name = Ident::new(value, Span::call_site());
            quote! { #facade::Theme::#name() }
        }
        Literal::Duration(_) => unreachable!("transitions are emitted after static setters"),
        Literal::Identifier(_) | Literal::Percent(_) | Literal::Fraction(_) | Literal::List(_) => {
            unreachable!("only layout properties take these values")
        }
        Literal::Int(_) | Literal::Expr(_) => {
            unreachable!("static checking normalizes integers and rejects expressions")
        }
    };
    let method = Ident::new(name, Span::call_site());
    Some(quote! { #handle.#method(#argument)?; })
}

fn transition(node: &CheckedNode, handle: &Ident, facade: &TokenStream) -> Option<TokenStream> {
    let property = node
        .properties
        .iter()
        .find(|property| property.name == PropertyName::Transition)?;
    let Literal::Duration(milliseconds) = property.value else {
        unreachable!()
    };
    let easing = node
        .properties
        .iter()
        .find(|property| property.name == PropertyName::Easing)
        .map(|property| {
            let Literal::Identifier(name) = &property.value else {
                unreachable!()
            };
            match name.as_str() {
                "linear" => "Linear",
                "ease_in" => "EaseIn",
                "ease_out" => "EaseOut",
                "ease_in_out" => "EaseInOut",
                _ => unreachable!(),
            }
        })
        .unwrap_or("EaseOut");
    let easing = Ident::new(easing, Span::call_site());
    Some(quote! {
        #handle.set_transition(#facade::Transition::new(
            ::core::time::Duration::from_millis(#milliseconds),
            #facade::Easing::#easing,
        ))?;
    })
}
