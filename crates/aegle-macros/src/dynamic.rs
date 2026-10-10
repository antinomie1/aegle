//! Dynamic documents: checked here, executed at run time by the shared engine.

use aegle_markup::{ElementKind, Type};
use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};

use crate::generate::Context;

/// Emits a builder that constructs the checked program once per thread,
/// with the elements it uses, and builds views with `loader::Program`.
pub(super) fn builder(cx: &Context<'_>) -> TokenStream {
    let (facade, loader) = (&cx.facade, &cx.loader);
    let program = cx.program;
    let mut root = &program.templates[0].root;
    while let ElementKind::Component(template) = root.kind {
        root = &program.templates[template].root;
    }
    let root_type = cx.handle_type(root.kind);
    let (parent_type, method) = if root.kind == ElementKind::Window {
        (quote! { #facade::app::App }, quote! { open })
    } else {
        (quote! { #facade::ui::Container }, quote! { build })
    };
    let states = &program.templates[0].states;
    let ids: Vec<Ident> = program
        .ids
        .iter()
        .map(|(id, _)| Ident::new(id, Span::call_site()))
        .collect();
    let id_types: Vec<_> = program
        .ids
        .iter()
        .map(|(_, kind)| cx.handle_type(*kind))
        .collect();
    let id_indices = 0..ids.len();
    let state_names: Vec<Ident> = states
        .iter()
        .map(|(name, ..)| Ident::new(name, Span::call_site()))
        .collect();
    let state_types: Vec<_> = states
        .iter()
        .map(|(_, ty, _)| rust_type(ty, loader))
        .collect();
    let state_indices = 0..states.len();
    let elements = (0..program.elements.len()).map(|index| cx.element(index));
    let view = format_ident!("__AegleView", span = Span::mixed_site());
    let parent = format_ident!("__aegle_parent", span = Span::mixed_site());
    let checked = crate::lower::program(program, &quote! { #loader::markup });
    quote! {{
        ::std::thread_local! {
            static __AEGLE_PROGRAM: #loader::Program = #loader::Program::from_checked(
                #checked,
                &#loader::Elements::new()#(.with::<#elements>())*,
            );
        }
        #[allow(dead_code, non_snake_case)]
        struct #view {
            pub root: #root_type,
            #(pub #ids: #id_types,)*
            #(pub #state_names: #loader::State<#state_types>,)*
        }
        move |#parent: &#parent_type| -> #facade::ui::Result<#view> {
            let view = __AEGLE_PROGRAM.with(|program| program.#method(#parent))?;
            let typed = "markup handle types are fixed when compiled";
            ::core::result::Result::Ok(#view {
                root: view.root().typed().expect(typed),
                #(#ids: view.id(#id_indices).typed().expect(typed),)*
                #(#state_names: view.state_at(#state_indices),)*
            })
        }
    }}
}

/// Records, and lists of them, are held as untyped [`Data`](loader::Data).
fn rust_type(ty: &Type, loader: &TokenStream) -> TokenStream {
    match ty {
        Type::Bool => quote! { bool },
        Type::Int => quote! { i64 },
        Type::Float => quote! { f32 },
        Type::String => quote! { ::std::string::String },
        Type::List(item) if matches!(**item, Type::Record(_)) => quote! { #loader::Data },
        Type::List(item) => {
            let item = rust_type(item, loader);
            quote! { ::std::vec::Vec<#item> }
        }
        Type::Record(_) => quote! { #loader::Data },
    }
}
