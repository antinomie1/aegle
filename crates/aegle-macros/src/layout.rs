//! Setter calls for checked layout properties.

use aegle_markup::{CheckedProperty, PropertyName, Value as Literal};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::Ident;

/// `snake_case` markup identifiers name `CamelCase` Rust variants.
pub(crate) fn variant(name: &str) -> Ident {
    let camel: String = name
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or(String::new(), |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect();
    Ident::new(&camel, Span::call_site())
}

fn length(value: &Literal, f: &TokenStream) -> TokenStream {
    match value {
        Literal::Length(v) => quote! { #f::Length::Px(#v) },
        Literal::Percent(v) => quote! { #f::Length::Percent(#v) },
        Literal::Call(_, parts) => {
            let [Literal::Percent(percent), Literal::Length(px)] = parts[..] else {
                unreachable!("checked calc")
            };
            quote! { #f::Length::Calc { percent: #percent, px: #px } }
        }
        _ => quote! { #f::Length::Auto },
    }
}

/// One length, `[vertical, horizontal]` or `[top, right, bottom, left]`.
fn insets(value: &Literal, f: &TokenStream) -> TokenStream {
    let lengths: Vec<_> = match value {
        Literal::List(items) => items.iter().map(|item| length(item, f)).collect(),
        value => vec![length(value, f)],
    };
    match lengths.as_slice() {
        [all] => quote! { #f::Insets::all(#all) },
        [vertical, horizontal] => quote! { #f::Insets::symmetric(#horizontal, #vertical) },
        [top, right, bottom, left] => quote! { #f::Insets::new(#top, #right, #bottom, #left) },
        _ => unreachable!("checked edge lists"),
    }
}

fn items(value: &Literal) -> &[Literal] {
    match value {
        Literal::List(items) => items,
        value => std::slice::from_ref(value),
    }
}

fn track(value: &Literal, f: &TokenStream) -> TokenStream {
    match value {
        Literal::Length(v) => quote! { #f::Track::Px(#v) },
        Literal::Percent(v) => quote! { #f::Track::Percent(#v) },
        Literal::Fraction(v) => quote! { #f::Track::Fr(#v) },
        Literal::Identifier(name) => {
            let name = variant(name);
            quote! { #f::Track::#name }
        }
        Literal::Call(_, arguments) => match arguments[..] {
            [Literal::Length(min), Literal::Fraction(fr)] => {
                quote! { #f::Track::MinMax(#min, #fr) }
            }
            [Literal::Length(max)] => quote! { #f::Track::FitContent(#max) },
            _ => unreachable!("checked track function"),
        },
        _ => unreachable!("checked tracks"),
    }
}

/// A template item: a track, a line name or a repeat of those.
fn template_item(value: &Literal, f: &TokenStream) -> TokenStream {
    match value {
        Literal::String(name) => {
            quote! { #f::TemplateItem::Line(::std::string::String::from(#name)) }
        }
        Literal::Call(function, arguments) if function == "repeat" => {
            let count = match &arguments[0] {
                Literal::Number(n) => {
                    let n = *n as u16;
                    quote! { #f::Repeat::Count(#n) }
                }
                Literal::Identifier(name) => {
                    let name = variant(name);
                    quote! { #f::Repeat::#name }
                }
                _ => unreachable!("checked repeat count"),
            };
            let inner = arguments[1..].iter().map(|item| template_item(item, f));
            quote! { #f::TemplateItem::Repeat(#count, ::std::vec![#(#inner),*]) }
        }
        value => {
            let track = track(value, f);
            quote! { #f::TemplateItem::Track(#track) }
        }
    }
}

fn grid_line(value: &Literal, end: bool, f: &TokenStream) -> TokenStream {
    match value {
        Literal::Number(n) if end => {
            let n = *n as u16;
            quote! { #f::GridLine::Span(#n) }
        }
        Literal::Number(n) => {
            let n = *n as i16;
            quote! { #f::GridLine::Line(#n) }
        }
        Literal::String(name) => {
            quote! { #f::GridLine::Named(::std::string::String::from(#name), 1) }
        }
        _ => quote! { #f::GridLine::Auto },
    }
}

/// A line or name alone covers one track or the named area; a pair is
/// `[start, span or end name]`.
fn placement(value: &Literal, f: &TokenStream) -> TokenStream {
    match value {
        Literal::List(items) => {
            let (start, end) = (
                grid_line(&items[0], false, f),
                grid_line(&items[1], true, f),
            );
            quote! { #f::GridLines { start: #start, end: #end } }
        }
        Literal::String(name) => quote! { #f::GridLines::named(#name) },
        value => {
            let start = grid_line(value, false, f);
            quote! { #f::GridLines { start: #start, end: #f::GridLine::Span(1) } }
        }
    }
}

/// The setter call for a layout property, or `None` for other properties.
pub(super) fn setter(
    property: &CheckedProperty,
    handle: &Ident,
    f: &TokenStream,
) -> Option<TokenStream> {
    use PropertyName::*;
    let value = &property.value;
    let enumerated = |kind: &str| {
        let kind = Ident::new(kind, Span::call_site());
        let Literal::Identifier(name) = value else {
            unreachable!("checked identifier")
        };
        let name = variant(name);
        quote! { #f::#kind::#name }
    };
    let number = || {
        let Literal::Number(n) = value else {
            unreachable!("checked number")
        };
        *n
    };
    let call = match property.name {
        Width | Height | MinWidth | MinHeight | MaxWidth | MaxHeight | Basis => {
            let method = match property.name {
                Width => "set_width",
                Height => "set_height",
                MinWidth => "set_min_width",
                MinHeight => "set_min_height",
                MaxWidth => "set_max_width",
                MaxHeight => "set_max_height",
                _ => "set_basis",
            };
            let method = Ident::new(method, Span::call_site());
            let length = length(value, f);
            quote! { #handle.#method(#length) }
        }
        AspectRatio => {
            let ratio = number();
            quote! { #handle.set_aspect_ratio(::core::option::Option::Some(#ratio)) }
        }
        Shrink => {
            let shrink = number();
            quote! { #handle.set_shrink(#shrink) }
        }
        Padding => {
            let insets = insets(value, f);
            quote! { #handle.set_padding(#insets) }
        }
        Margin => {
            let insets = insets(value, f);
            quote! { #handle.set_margin(#insets) }
        }
        Inset => {
            let insets = insets(value, f);
            quote! { #handle.set_absolute(::core::option::Option::Some(#insets)) }
        }
        Gap => match value {
            Literal::List(items) => {
                let (row, column) = (length(&items[0], f), length(&items[1], f));
                quote! { #handle.set_gaps(#column, #row) }
            }
            value => {
                let gap = length(value, f);
                quote! { #handle.set_gap(#gap) }
            }
        },
        Direction => {
            let direction = enumerated("Direction");
            quote! { #handle.set_direction(#direction) }
        }
        LayoutDirection => {
            let direction = enumerated("LayoutDirection");
            quote! { #handle.set_layout_direction(::core::option::Option::Some(#direction)) }
        }
        Wrap => {
            let wrap = enumerated("Wrap");
            quote! { #handle.set_wrap(#wrap) }
        }
        Flow => {
            let flow = enumerated("Flow");
            quote! { #handle.set_flow(#flow) }
        }
        Align | AlignSelf | JustifySelf | JustifyItems => {
            let method = match property.name {
                Align => "set_align_items",
                AlignSelf => "set_align_self",
                JustifySelf => "set_justify_self",
                _ => "set_justify_items",
            };
            let method = Ident::new(method, Span::call_site());
            let align = enumerated("Align");
            quote! { #handle.#method(::core::option::Option::Some(#align)) }
        }
        Justify | AlignContent => {
            let method = if property.name == Justify {
                "set_justify_content"
            } else {
                "set_align_content"
            };
            let method = Ident::new(method, Span::call_site());
            let justify = enumerated("Justify");
            quote! { #handle.#method(::core::option::Option::Some(#justify)) }
        }
        Columns | Rows => {
            let method = if property.name == Columns {
                "set_column_template"
            } else {
                "set_row_template"
            };
            let method = Ident::new(method, Span::call_site());
            let items = items(value).iter().map(|item| template_item(item, f));
            quote! { #handle.#method(&[#(#items),*]) }
        }
        AutoColumns | AutoRows => {
            let method = if property.name == AutoColumns {
                "set_auto_columns"
            } else {
                "set_auto_rows"
            };
            let method = Ident::new(method, Span::call_site());
            let tracks = items(value).iter().map(|item| track(item, f));
            quote! { #handle.#method(&[#(#tracks),*]) }
        }
        Areas => {
            let rows = items(value).iter().map(|row| {
                let Literal::String(row) = row else {
                    unreachable!("checked area rows")
                };
                quote! { #row }
            });
            quote! { #handle.set_areas(&[#(#rows),*]) }
        }
        GridArea => {
            let Literal::String(name) = value else {
                unreachable!("checked area name")
            };
            quote! { #handle.set_grid_area(#name) }
        }
        GridColumn | GridRow => {
            let method = if property.name == GridColumn {
                "set_grid_column"
            } else {
                "set_grid_row"
            };
            let method = Ident::new(method, Span::call_site());
            let placement = placement(value, f);
            quote! { #handle.#method(#placement) }
        }
        _ => return None,
    };
    Some(quote! { #call?; })
}
