//! Code for presented geometry and transition timing properties.

use aegle_markup::{CheckedNode, CheckedProperty, PropertyName, Value as Literal};
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;

/// A literal offset, scale or rotation setter; it keeps the other parts of
/// the node's target geometry.
pub(super) fn geometry(
    property: &CheckedProperty,
    handle: &Ident,
    facade: &TokenStream,
) -> Option<TokenStream> {
    use PropertyName::*;
    let (Literal::Length(value) | Literal::Number(value)) = property.value else {
        return None;
    };
    Some(match property.name {
        OffsetX => quote! {
            #handle.set_offset(#facade::Point::new(#value, #handle.offset()?.y))?;
        },
        OffsetY => quote! {
            #handle.set_offset(#facade::Point::new(#handle.offset()?.x, #value))?;
        },
        Scale => quote! {
            #handle.set_transform(#facade::Transform { scale: #value, ..#handle.transform()? })?;
        },
        Rotation => {
            let radians = value.to_radians();
            quote! {
                #handle.set_transform(#facade::Transform { rotation: #radians, ..#handle.transform()? })?;
            }
        }
        _ => return None,
    })
}

fn easing(name: &str) -> Ident {
    let variant = match name {
        "linear" => "Linear",
        "ease_in" => "EaseIn",
        "ease_in_out" => "EaseInOut",
        _ => "EaseOut",
    };
    Ident::new(variant, Span::call_site())
}

fn timing(milliseconds: u64, easing: &Ident, facade: &TokenStream) -> TokenStream {
    quote! {
        #facade::Transition::new(::core::time::Duration::from_millis(#milliseconds), #facade::Easing::#easing)
    }
}

/// The node-wide `transition`, then each per-property timing over it,
/// installed after the static setters so creation does not animate.
pub(super) fn transitions(
    node: &CheckedNode,
    handle: &Ident,
    facade: &TokenStream,
) -> Option<TokenStream> {
    let mut calls = Vec::new();
    let find = |name| node.properties.iter().find(|p| p.name == name);
    if let Some(property) = find(PropertyName::Transition) {
        let Literal::Duration(milliseconds) = property.value else {
            unreachable!()
        };
        let curve = match find(PropertyName::Easing).map(|p| &p.value) {
            Some(Literal::Identifier(name)) => easing(name),
            _ => easing("ease_out"),
        };
        let timing = timing(milliseconds, &curve, facade);
        calls.push(quote! { #handle.set_transition(#timing)?; });
    }
    for property in &node.properties {
        let target = match property.name {
            PropertyName::PaintTransition => "Paint",
            PropertyName::OffsetTransition => "Offset",
            PropertyName::ScaleTransition => "Scale",
            PropertyName::RotationTransition => "Rotation",
            _ => continue,
        };
        let (milliseconds, curve) = match &property.value {
            Literal::Duration(milliseconds) => (*milliseconds, easing("ease_out")),
            Literal::List(items) => match &items[..] {
                [Literal::Duration(milliseconds), Literal::Identifier(name)] => {
                    (*milliseconds, easing(name))
                }
                _ => unreachable!("checked timing"),
            },
            _ => unreachable!("checked timing"),
        };
        let timing = timing(milliseconds, &curve, facade);
        let target = Ident::new(target, Span::call_site());
        calls.push(quote! {
            #handle.set_property_transition(#facade::TransitionProperty::#target, ::core::option::Option::Some(#timing))?;
        });
    }
    (!calls.is_empty()).then(|| quote! { #(#calls)* })
}
