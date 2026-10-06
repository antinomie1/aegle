//! Dynamic documents: checked here, executed at run time by the shared engine.

use std::{fs::File, io::Read, path::Path};

use aegle_markup::{ElementKind, Kind, Limits, Type};
use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};
use syn::LitStr;

use crate::generate::handle_type;

/// Checks the entry and its imports, then emits a builder that constructs the
/// checked program once per thread and builds views with `loader::Program`.
pub(super) fn builder(
    entry: &str,
    manifest: &Path,
    file: &LitStr,
    facade: &TokenStream,
) -> syn::Result<TokenStream> {
    let diagnostic = |message: String| syn::Error::new(file.span(), message);
    let limit = Limits::default().max_source_bytes as u64;
    let (program, files) = aegle_markup::compile(entry, &mut |path| {
        let mut source = String::new();
        File::open(manifest.join(path))
            .and_then(|file| file.take(limit + 1).read_to_string(&mut source))
            .map_err(|error| error.to_string())?;
        Ok(source)
    })
    .map_err(|error| diagnostic(error.0))?;
    let dependencies = files
        .iter()
        .map(|source| {
            let path = manifest.join(&source.path);
            let path = path
                .to_str()
                .ok_or_else(|| diagnostic("markup path is not UTF-8".into()))?;
            Ok(LitStr::new(path, file.span()))
        })
        .collect::<syn::Result<Vec<_>>>()?;
    let mut root = &program.templates[0].root;
    let kind = loop {
        match root.kind {
            ElementKind::Builtin(kind) => break kind,
            ElementKind::Component(template) => root = &program.templates[template].root,
        }
    };
    let root_type = handle_type(kind, facade);
    let (parent_type, method) = if kind == Kind::Window {
        (quote! { #facade::App }, quote! { open })
    } else {
        (quote! { #facade::Container }, quote! { build })
    };
    let states = &program.templates[0].states;
    if let Some((name, _)) = program
        .ids
        .iter()
        .find(|(id, _)| states.iter().any(|(s, ..)| s == id))
    {
        return Err(diagnostic(format!(
            "`{name}` names both a control and a state"
        )));
    }
    let ids: Vec<Ident> = program
        .ids
        .iter()
        .map(|(id, _)| Ident::new(id, Span::call_site()))
        .collect();
    let id_types: Vec<_> = program
        .ids
        .iter()
        .map(|(_, kind)| handle_type(*kind, facade))
        .collect();
    let id_indices = 0..ids.len();
    let state_names: Vec<Ident> = states
        .iter()
        .map(|(name, ..)| Ident::new(name, Span::call_site()))
        .collect();
    let state_types: Vec<_> = states
        .iter()
        .map(|(_, ty, _)| rust_type(ty, &quote! { #facade::loader }))
        .collect();
    let state_indices = 0..states.len();
    let view = format_ident!("__AegleView", span = Span::mixed_site());
    let parent = format_ident!("__aegle_parent", span = Span::mixed_site());
    let loader = quote! { #facade::loader };
    let checked = crate::lower::program(&program, &quote! { #loader::markup });
    Ok(quote! {{
        // Imports are tracked for recompilation like the entry file.
        #(const _: &str = ::core::include_str!(#dependencies);)*
        ::std::thread_local! {
            static __AEGLE_PROGRAM: #loader::Program = #loader::Program::from_checked(#checked);
        }
        #[allow(dead_code, non_snake_case)]
        struct #view {
            pub root: #root_type,
            #(pub #ids: #id_types,)*
            #(pub #state_names: #loader::State<#state_types>,)*
        }
        move |#parent: &#parent_type| -> #facade::Result<#view> {
            let view = __AEGLE_PROGRAM.with(|program| program.#method(#parent))?;
            let typed = "markup handle kinds are fixed when compiled";
            ::core::result::Result::Ok(#view {
                root: <#root_type as #loader::FromHandle>::from_handle(view.root()).expect(typed),
                #(#ids: <#id_types as #loader::FromHandle>::from_handle(view.id(#id_indices)).expect(typed),)*
                #(#state_names: view.state_at(#state_indices),)*
            })
        }
    }})
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
