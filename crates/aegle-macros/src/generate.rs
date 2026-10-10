//! Static documents: direct construction through each element's glue and
//! the loader's node-property setters, without the runtime engine.

use aegle_markup::{
    Bound, Child, Element, ElementKind, ElementSpec, Program, Prop, PropertyName, Value, ValueType,
};
use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};

use crate::lower::{value, variant};

/// What generated code needs: the facade, the loader, the program's
/// elements by name and their specs.
pub(crate) struct Context<'a> {
    pub facade: TokenStream,
    pub loader: TokenStream,
    pub program: &'a Program,
    pub specs: &'a [ElementSpec<'static>],
    pub span: Span,
}

impl Context<'_> {
    /// The element type of `Program::elements[index]`, resolved where `ui!` is called.
    pub(crate) fn element(&self, index: usize) -> Ident {
        Ident::new(&self.program.elements[index], self.span)
    }
    fn spec(&self, index: usize) -> &ElementSpec<'static> {
        let name = &self.program.elements[index];
        self.specs.iter().find(|spec| spec.name == name).unwrap()
    }
    /// The Rust type of a handle to `kind`.
    pub(crate) fn handle_type(&self, kind: ElementKind) -> TokenStream {
        let (facade, loader) = (&self.facade, &self.loader);
        match kind {
            ElementKind::Window => quote! { #facade::Window },
            ElementKind::Control(index) => {
                let element = self.element(index);
                quote! { <#element as #loader::Element>::Handle }
            }
            ElementKind::Component(_) => unreachable!("components are expanded"),
        }
    }
}

/// Only checked static programs reach generation: one template whose
/// elements have literal properties and element children.
pub(super) fn builder(cx: &Context<'_>) -> TokenStream {
    let (facade, loader) = (&cx.facade, &cx.loader);
    let root = &cx.program.templates[0].root;
    let root_handle = format_ident!("__aegle_node_0", span = Span::mixed_site());
    let parent = format_ident!("__aegle_parent", span = Span::mixed_site());
    let view = format_ident!("__AegleView", span = Span::mixed_site());
    let root_type = cx.handle_type(root.kind);
    let window = root.kind == ElementKind::Window;
    let (parent_type, create_root, cleanup) = if window {
        let literal = |name| literal(root, name);
        let title = match literal(PropertyName::Title) {
            Some(Value::String(title)) => title.as_str(),
            _ => "Aegle",
        };
        let size = [
            (PropertyName::Width, "width"),
            (PropertyName::Height, "height"),
        ];
        let size = size.into_iter().filter_map(|(name, field)| {
            let Some(Value::Length(value)) = literal(name) else {
                return None;
            };
            let (field, value) = (Ident::new(field, Span::call_site()), *value as u32);
            Some(quote! { #field: #value, })
        });
        (
            quote! { #facade::App },
            quote! {
                #parent.window_with_options(#title, #facade::WindowOptions {
                    #(#size)*
                    ..::core::default::Default::default()
                })?
            },
            quote! { #root_handle.close()?; },
        )
    } else {
        let ElementKind::Control(index) = root.kind else {
            unreachable!("static roots are elements")
        };
        let element = cx.element(index);
        (
            quote! { #facade::Container },
            create(cx, root, index, &quote! { #parent }),
            quote! { <#element as #loader::Element>::node(&#root_handle).remove(); },
        )
    };
    let mut output = Output::default();
    output.visit(cx, root, &root_handle);
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
            let #root_handle = #create_root;
            let __aegle_result = (|| -> #facade::Result<#view> {
                #(#creations)*
                #(#setters)*
                #(#transitions)*
                ::core::result::Result::Ok(#view {
                    root: ::core::clone::Clone::clone(&#root_handle),
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

fn literal(element: &Element, name: PropertyName) -> Option<&Value> {
    element
        .properties
        .iter()
        .find_map(|(prop, bound)| match bound {
            Bound::Literal(value) if *prop == Prop::Node(name) => Some(value),
            _ => None,
        })
}

/// The constructor call of an element node with its literal arguments.
fn create(cx: &Context<'_>, element: &Element, index: usize, parent: &TokenStream) -> TokenStream {
    let loader = &cx.loader;
    let spec = cx.spec(index);
    let arguments = element.properties.iter().filter_map(|(prop, bound)| {
        let (Prop::Element(property), Bound::Literal(value)) = (prop, bound) else {
            return None;
        };
        let property_spec = &spec.properties[*property];
        property_spec.new.then(|| {
            let arg = arg(property_spec.ty, value, loader);
            quote! { (#property, #arg) }
        })
    });
    let element = cx.element(index);
    quote! { <#element as #loader::Element>::create(&#parent, &[#(#arguments),*]) }
}

/// An element literal as the `Arg` the loader would pass at run time.
fn arg(ty: ValueType<'_>, value: &Value, loader: &TokenStream) -> TokenStream {
    let arg = quote! { #loader::Arg };
    match (ty, value) {
        (_, Value::Bool(value)) => quote! { #arg::Bool(#value) },
        (ValueType::Int(..), Value::Number(value)) => {
            let value = *value as i64;
            quote! { #arg::Int(#value) }
        }
        (_, Value::Number(value) | Value::Length(value)) => {
            let value = f64::from(*value);
            quote! { #arg::Float(#value) }
        }
        (_, Value::Percent(value)) => {
            let value = f64::from(*value) / 100.0;
            quote! { #arg::Float(#value) }
        }
        (_, Value::String(value) | Value::Identifier(value)) => quote! { #arg::Str(#value) },
        (_, Value::Color([r, g, b, a])) => {
            quote! { #arg::Color(#loader::__private::Color::rgba(#r, #g, #b, #a)) }
        }
        _ => unreachable!("checked element literal"),
    }
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
    fn visit(&mut self, cx: &Context<'_>, element: &Element, handle: &Ident) {
        let (facade, loader) = (&cx.facade, &cx.loader);
        let m = quote! { #loader::markup };
        let p = quote! { #loader::__private };
        if let Some(id) = element.id {
            let field = Ident::new(&cx.program.ids[id].0, Span::call_site());
            let ty = cx.handle_type(element.kind);
            self.fields.push(quote! { pub #field: #ty, });
            self.values
                .push(quote! { #field: ::core::clone::Clone::clone(&#handle), });
        }
        let (node, glue) = match element.kind {
            ElementKind::Control(index) => {
                let name = cx.element(index);
                let glue = quote! { <#name as #loader::Element> };
                (
                    quote! { #glue::node(&#handle) },
                    Some((glue, cx.spec(index))),
                )
            }
            _ => (quote! { &**#handle }, None),
        };
        let mut timed = Vec::new();
        for (prop, bound) in &element.properties {
            let Bound::Literal(literal) = bound else {
                unreachable!("static documents hold literals")
            };
            match (prop, &glue) {
                (Prop::Node(PropertyName::Theme), _) => {
                    let Value::Identifier(theme) = literal else {
                        unreachable!("checked theme")
                    };
                    let theme = Ident::new(theme, Span::call_site());
                    self.setters
                        .push(quote! { #handle.set_theme(#facade::Theme::#theme()); });
                }
                (
                    Prop::Node(PropertyName::Title | PropertyName::Width | PropertyName::Height),
                    None,
                ) => {}
                (Prop::Node(name), _) if timing(*name) => {
                    let (name, value) = (variant(name), value(literal, &m));
                    timed.push(quote! { (#m::PropertyName::#name, &#value) });
                }
                (Prop::Node(name), _) => {
                    let (name, value) = (variant(name), value(literal, &m));
                    self.setters
                        .push(quote! { #p::apply(#node, #m::PropertyName::#name, &#value)?; });
                }
                (Prop::Element(index), Some((glue, spec))) => {
                    let property = &spec.properties[*index];
                    if !property.new {
                        let arg = arg(property.ty, literal, loader);
                        self.setters
                            .push(quote! { #glue::set(&#handle, #index, #arg); });
                    }
                }
                (Prop::Element(_), None) => unreachable!("windows have no element properties"),
            }
        }
        if !timed.is_empty() {
            self.transitions
                .push(quote! { #p::transitions(#node, &[#(#timed),*]); });
        }
        for (index, child) in element.children.iter().enumerate() {
            let Child::Element(child) = child else {
                unreachable!("static documents hold elements")
            };
            let ElementKind::Control(kind) = child.kind else {
                unreachable!("static children are elements")
            };
            self.count += 1;
            let child_handle =
                format_ident!("__aegle_node_{}", self.count, span = Span::mixed_site());
            let parent = match &glue {
                Some((glue, _)) => quote! { #glue::parent(&#handle, #index) },
                None => quote! { ::core::clone::Clone::clone(&*#handle) },
            };
            let constructor = create(cx, child, kind, &parent);
            self.creations
                .push(quote! { let #child_handle = #constructor; });
            self.visit(cx, child, &child_handle);
        }
    }
}

fn timing(name: PropertyName) -> bool {
    use PropertyName::*;
    matches!(
        name,
        Transition
            | PaintTransition
            | OffsetTransition
            | ScaleTransition
            | RotationTransition
            | ShadowTransition
            | OpacityTransition
    )
}
