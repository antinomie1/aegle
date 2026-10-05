use aegle_markup::{CheckedDocument, CheckedNode, CheckedProperty, Kind, PropertyName, Value};
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
    let create_root = constructor(root, &parent, facade);
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
            let #root_handle = #create_root?;
            let __aegle_result = (|| -> #facade::Result<#view> {
                #(#creations)*
                #(#setters)*
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
        for child in &node.children {
            self.count += 1;
            let child_handle =
                format_ident!("__aegle_node_{}", self.count, span = Span::mixed_site());
            let constructor = constructor(child, handle, facade);
            self.creations
                .push(quote! { let #child_handle = #constructor?; });
            self.visit(child, &child_handle, facade);
        }
    }
}

fn handle_type(kind: Kind, facade: &TokenStream) -> TokenStream {
    let name = match kind {
        Kind::Window => "Window",
        Kind::Row | Kind::Column => "Container",
        Kind::Text => "Label",
        Kind::Button => "Button",
        Kind::TextField | Kind::TextArea => "TextField",
    };
    let name = Ident::new(name, Span::call_site());
    quote! { #facade::#name }
}

fn constructor(node: &CheckedNode, parent: &Ident, facade: &TokenStream) -> TokenStream {
    let text = node.properties.iter().find_map(|property| {
        if matches!(property.name, PropertyName::Title | PropertyName::Text) {
            let Value::String(value) = &property.value else {
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
            let Value::Length(value) = property.value else {
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
    match node.kind {
        Kind::Row => quote! { #parent.row() },
        Kind::Column => quote! { #parent.column() },
        Kind::Text => quote! { #parent.text(#text) },
        Kind::Button => quote! { #parent.button(#text) },
        Kind::TextField => quote! { #parent.text_field(#text) },
        Kind::TextArea => quote! { #parent.text_area(#text) },
        Kind::Window => unreachable!(),
    }
}

fn setter(
    kind: Kind,
    property: &CheckedProperty,
    handle: &Ident,
    facade: &TokenStream,
) -> Option<TokenStream> {
    use PropertyName::*;
    let name = match property.name {
        Title | Text => return None,
        Width | Height if kind == Kind::Window => return None,
        Width => "set_width",
        Height => "set_height",
        MinWidth => "set_min_width",
        MinHeight => "set_min_height",
        Padding => "set_padding",
        Gap => "set_gap",
        Grow => "set_grow",
        Visible => "set_visible",
        Enabled => "set_enabled",
        Label => "set_accessible_label",
        ReadOnly => "set_read_only",
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
    };
    let argument = match &property.value {
        Value::String(value) => quote! { #value },
        Value::Bool(value) => quote! { #value },
        Value::Color([red, green, blue, alpha]) => {
            quote! { #facade::Color::rgba(#red, #green, #blue, #alpha) }
        }
        Value::Number(value) | Value::Length(value) => {
            if matches!(property.name, Width | Height) {
                quote! { ::core::option::Option::Some(#value) }
            } else {
                quote! { #value }
            }
        }
        Value::Identifier(value) if property.name == Theme => {
            let name = Ident::new(value, Span::call_site());
            quote! { #facade::Theme::#name() }
        }
        Value::Identifier(_) => quote! { ::core::option::Option::None },
    };
    let method = Ident::new(name, Span::call_site());
    Some(quote! { #handle.#method(#argument)?; })
}
