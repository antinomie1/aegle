//! Dynamic documents: checked here, executed at run time by the shared engine.

use aegle_markup::{Element, ElementKind, Program, Type};
use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};

use crate::generate::Context;

/// Emits a builder that constructs the checked program once per thread,
/// with the elements it uses, and builds views with `loader::Program`.
pub(super) fn builder(cx: &Context<'_>) -> TokenStream {
    let (facade, loader) = (&cx.facade, &cx.loader);
    let program = cx.program;
    let root = body_root(program, &program.templates[0].root);
    let root_type = cx.handle_type(root.kind);
    let (parent_type, method) = if root.kind == ElementKind::Window {
        (quote! { #facade::app::App }, quote! { open })
    } else {
        (quote! { #facade::ui::Container }, quote! { build })
    };
    let ids: Vec<Ident> = program
        .ids
        .iter()
        .map(|(id, _)| Ident::new(id, Span::call_site()))
        .collect();
    // A named component instance is a struct of its root control and states,
    // one struct per component.
    let mut structs = Vec::new();
    let mut declared = Vec::new();
    let (mut id_types, mut id_values) = (Vec::new(), Vec::new());
    for (index, (_, kind)) in program.ids.iter().enumerate() {
        let ElementKind::Component(template) = *kind else {
            id_types.push(cx.handle_type(*kind));
            id_values.push(quote! { view.id(#index).typed().expect(typed) });
            continue;
        };
        let component = &program.templates[template];
        let name = format_ident!(
            "__AegleInstance{}",
            component.name,
            span = Span::mixed_site()
        );
        let (names, types) = states(&component.states, loader);
        let indices = 0..names.len();
        if !declared.contains(&template) {
            declared.push(template);
            let root = cx.handle_type(body_root(program, &component.root).kind);
            structs.push(quote! {
                #[allow(dead_code, non_snake_case)]
                struct #name {
                    pub root: #root,
                    #(pub #names: #loader::State<#types>,)*
                }
            });
        }
        id_types.push(quote! { #name });
        id_values.push(quote! {{
            let instance = view.instance_at(#index);
            #name {
                root: instance.root().typed().expect(typed),
                #(#names: instance.state_at(#indices),)*
            }
        }});
    }
    let (state_names, state_types) = states(&program.templates[0].states, loader);
    let state_indices = 0..state_names.len();
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
        #(#structs)*
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
                #(#ids: #id_values,)*
                #(#state_names: view.state_at(#state_indices),)*
            })
        }
    }}
}

/// The element a component body, or a use of one, creates first: its root
/// control, or the window.
fn body_root<'p>(program: &'p Program, mut element: &'p Element) -> &'p Element {
    while let ElementKind::Component(template) = element.kind {
        element = &program.templates[template].root;
    }
    element
}

/// Field names and Rust types of the typed handles to `states`.
fn states(
    states: &[(String, Type, aegle_markup::Expr)],
    loader: &TokenStream,
) -> (Vec<Ident>, Vec<TokenStream>) {
    states
        .iter()
        .map(|(name, ty, _)| (Ident::new(name, Span::call_site()), rust_type(ty, loader)))
        .unzip()
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
