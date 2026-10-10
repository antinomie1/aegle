//! The code one definition expands to: its `Element` implementation, an
//! optional marker type and the spec macro `ui!` resolves by name.

use aegle_markup::{Children, ElementSpec, ValueType};
use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::Ident;

use super::Def;

impl Def {
    /// The `Element` implementation, a marker type when the element names
    /// another handle type, and the spec macro `ui!` resolves by name.
    pub(crate) fn expand(&self, l: &TokenStream) -> TokenStream {
        let Def {
            attrs, vis, name, ..
        } = self;
        let p = quote! { #l::__private };
        let (marker, handle) = match &self.handle {
            Some(handle) => (
                quote! {
                    #(#attrs)*
                    #[derive(Clone, Copy, Debug)]
                    #vis struct #name;
                },
                handle.to_token_stream(),
            ),
            None => (TokenStream::new(), name.to_token_stream()),
        };
        let spec = spec_tokens(&self.spec(), &quote! { #l::markup });
        let (parent, body) = &self.create;
        let arguments = self
            .properties
            .iter()
            .enumerate()
            .filter_map(|(index, property)| {
                let default = property.new.as_ref()?;
                let (name, (ty, read)) = (&property.name, rust(property.ty, &p));
                let value = match default {
                    Some(default) => {
                        quote! { __find(#index).map_or(#default as #ty, |a| a.#read()) }
                    }
                    None => quote! { __find(#index).unwrap().#read() },
                };
                Some(quote! { let #name: #ty = #value; })
            });
        let setters = self
            .properties
            .iter()
            .enumerate()
            .filter_map(|(index, property)| {
                let setter = property.set.as_ref()?;
                let read = rust(property.ty, &p).1;
                Some(quote! { #index => #p::set(handle, value.#read(), #setter), })
            });
        let events = self.events.iter().enumerate().map(|(index, (_, listen))| {
            quote! { #index => #p::listen(handle, run, #listen), }
        });
        let fields = self.fields.iter().enumerate().map(|(index, (_, ty, get))| {
            let data = match ty {
                ValueType::Bool => quote! { Bool(#p::get(handle, #get)) },
                ValueType::Int(..) => quote! { Int(#p::get(handle, #get)) },
                ValueType::String | ValueType::Line => quote! {
                    String(::std::convert::Into::into(
                        #p::get::<_, ::std::string::String>(handle, #get),
                    ))
                },
                _ => quote! { Float(#p::get::<_, f64>(handle, #get) as f32) },
            };
            quote! { #index => #l::Data::#data, }
        });
        let place = match &self.place {
            Some(place) => quote! { #p::place(handle, child, #place) },
            None => quote! { #p::Container(::core::clone::Clone::clone(Self::node(handle))) },
        };
        let tokens = &self.tokens;
        quote! {
            #marker
            impl #l::Element for #name {
                type Handle = #handle;
                const SPEC: #l::markup::ElementSpec<'static> = #spec;
                #[allow(unused_variables)]
                fn create(
                    #parent: &#p::Container,
                    args: &[(usize, #l::Arg<'_>)],
                ) -> #handle {
                    #[allow(unused)]
                    let __find = |index: usize| {
                        args.iter().find(|(i, _)| *i == index).map(|(_, a)| *a)
                    };
                    #(#arguments)*
                    #body
                }
                fn node(handle: &#handle) -> &#p::Node {
                    handle
                }
                #[allow(unused_variables)]
                fn set(handle: &#handle, property: usize, value: #l::Arg<'_>) {
                    match property {
                        #(#setters)*
                        _ => ::core::unreachable!("checked element property"),
                    }
                }
                #[allow(unused_variables)]
                fn get(handle: &#handle, field: usize) -> #l::Data {
                    match field {
                        #(#fields)*
                        _ => ::core::unreachable!("checked element field"),
                    }
                }
                #[allow(unused_variables)]
                fn listen(
                    handle: &#handle,
                    event: usize,
                    run: ::std::boxed::Box<dyn Fn() -> #p::Result>,
                ) {
                    match event {
                        #(#events)*
                        _ => ::core::unreachable!("checked element event"),
                    }
                }
                #[allow(unused_variables)]
                fn parent(handle: &#handle, child: usize) -> #p::Container {
                    #place
                }
            }
            #[doc(hidden)]
            #[macro_export]
            macro_rules! #name {
                ([$($callback:tt)*] $($state:tt)*) => {
                    $($callback)*! { { #tokens } $($state)* }
                };
            }
        }
    }
}

/// The Rust type a value type arrives as, and the `Arg` method reading it.
fn rust(ty: ValueType<'_>, p: &TokenStream) -> (TokenStream, Ident) {
    let (ty, read) = match ty {
        ValueType::Bool => (quote! { bool }, "bool"),
        ValueType::Int(..) => (quote! { i64 }, "i64"),
        ValueType::Float(..) => (quote! { f64 }, "f64"),
        ValueType::Fraction | ValueType::Length => (quote! { f32 }, "f32"),
        ValueType::Color => (quote! { #p::Color }, "color"),
        _ => (quote! { &str }, "str"),
    };
    (ty, Ident::new(read, Span::call_site()))
}

fn spec_tokens(spec: &ElementSpec<'_>, m: &TokenStream) -> TokenStream {
    let name = spec.name;
    let layout = Ident::new(&format!("{:?}", spec.layout), Span::call_site());
    let children = match spec.children {
        Children::Any => quote! { Any },
        Children::Only(name) => quote! { Only(#name) },
        Children::Exactly(count) => quote! { Exactly(#count) },
    };
    let parent = match spec.parent {
        Some(parent) => quote! { ::core::option::Option::Some(#parent) },
        None => quote! { ::core::option::Option::None },
    };
    let styles = spec.styles.0;
    let ty = |ty: &ValueType<'_>| match ty {
        ValueType::Int(min, max) => quote! { #m::ValueType::Int(#min, #max) },
        ValueType::Float(min, max) => quote! { #m::ValueType::Float(#min, #max) },
        ValueType::Choice(choices) => quote! { #m::ValueType::Choice(&[#(#choices),*]) },
        other => {
            let name = Ident::new(&format!("{other:?}"), Span::call_site());
            quote! { #m::ValueType::#name }
        }
    };
    let properties = spec.properties.iter().map(|p| {
        let (name, ty, new, set, required) = (p.name, ty(&p.ty), p.new, p.set, p.required);
        quote! {
            #m::PropertySpec { name: #name, ty: #ty, new: #new, set: #set, required: #required }
        }
    });
    let events = spec.events;
    let fields = spec.fields.iter().map(|(name, t)| {
        let t = ty(t);
        quote! { (#name, #t) }
    });
    quote! {
        #m::ElementSpec {
            name: #name,
            layout: #m::Layout::#layout,
            children: #m::Children::#children,
            parent: #parent,
            styles: #m::Styles(#styles),
            properties: &[#(#properties),*],
            events: &[#(#events),*],
            fields: &[#(#fields),*],
        }
    }
}
