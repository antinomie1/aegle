//! Compile `.aegle` files into ordinary retained control construction.
//!
//! Parsing and checking run in the compiler. A static document becomes only
//! handle constructors and property setters, without a markup parser, dynamic
//! component registry or named-node lookup table. A document with states,
//! bindings, events, `if`/`for` blocks, components or imports becomes Rust
//! code that constructs its checked program; `aegle::loader` executes it,
//! without a markup parser or checker in the binary.

mod dynamic;
mod element;
mod generate;
mod lower;

use std::{fs::File, io::Read, path::Path};

use aegle_markup::{ElementSpec, Sources};
use proc_macro::TokenStream;
use proc_macro2::{Delimiter, TokenStream as Tokens, TokenTree};
use quote::quote;
use syn::{Expr, ExprLit, Ident, Lit, LitStr, Token, parse::Parse, parse::ParseStream};

/// Expands the facade's `ui!`, which passes its own path ahead of the
/// arguments: `$crate; parent, "view.aegle"`.
#[doc(hidden)]
#[proc_macro]
pub fn __ui(input: TokenStream) -> TokenStream {
    let (facade, tokens) = split_path(input.into());
    let arguments = match syn::parse2::<Arguments>(tokens.clone()) {
        Ok(arguments) => arguments,
        Err(error) => return error.into_compile_error().into(),
    };
    let start = || -> syn::Result<Tokens> {
        let (_, sources) = sources(&arguments.file)?;
        let names = sources.elements();
        let span = arguments.file.span();
        let names: Vec<Ident> = names.iter().map(|name| Ident::new(name, span)).collect();
        resume(&facade, &[], &names, tokens, &arguments)
    };
    start()
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// One step of `ui!`: an element's spec macro forwards its definition here,
/// ahead of the names still to resolve and the original arguments.
#[doc(hidden)]
#[proc_macro]
pub fn __ui_resume(input: TokenStream) -> TokenStream {
    let mut specs = Vec::new();
    let mut names = Vec::new();
    let (mut facade, mut arguments) = (Tokens::new(), Tokens::new());
    for tree in Tokens::from(input) {
        let TokenTree::Group(group) = tree else {
            unreachable!("ui! state is grouped")
        };
        match group.delimiter() {
            Delimiter::Brace => specs.push(group.stream()),
            Delimiter::Bracket => names = group.stream().into_iter().collect(),
            _ => (facade, arguments) = split_path(group.stream()),
        }
    }
    let step = || -> syn::Result<Tokens> {
        let parsed = syn::parse2::<Arguments>(arguments.clone())?;
        let names: Vec<Ident> = names
            .into_iter()
            .map(|name| syn::parse2(name.into()))
            .collect::<syn::Result<_>>()?;
        resume(&facade, &specs, &names, arguments, &parsed)
    };
    step().unwrap_or_else(syn::Error::into_compile_error).into()
}

/// Asks the next unresolved element for its spec, or expands once every
/// spec has arrived.
fn resume(
    facade: &Tokens,
    specs: &[Tokens],
    names: &[Ident],
    arguments: Tokens,
    parsed: &Arguments,
) -> syn::Result<Tokens> {
    if let Some((next, rest)) = names.split_first() {
        return Ok(quote! {
            #next! { [#facade::__ui_resume] #({#specs})* [#(#rest)*] (#facade; #arguments) }
        });
    }
    let specs = specs
        .iter()
        .map(|spec| element::parse_spec(spec.clone()))
        .collect::<syn::Result<Vec<_>>>()?;
    expand(facade, parsed, &specs)
}

/// Expands the loader's `element!`, which passes its own path ahead of the
/// definitions: `$crate; pub Name { ... }`.
#[doc(hidden)]
#[proc_macro]
pub fn __element(input: TokenStream) -> TokenStream {
    let (loader, tokens) = split_path(input.into());
    let defs = match syn::parse2::<element::Defs>(tokens) {
        Ok(defs) => defs,
        Err(error) => return error.into_compile_error().into(),
    };
    defs.0
        .iter()
        .map(|def| def.expand(&loader))
        .collect::<Tokens>()
        .into()
}

struct Arguments {
    parent: Option<Expr>,
    file: LitStr,
}

impl Parse for Arguments {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let first: Expr = input.parse()?;
        let has_parent = if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            !input.is_empty()
        } else {
            false
        };
        let (parent, file) = if has_parent {
            (Some(first), input.parse()?)
        } else if let Expr::Lit(ExprLit {
            lit: Lit::Str(file),
            ..
        }) = first
        {
            (None, file)
        } else {
            return Err(syn::Error::new_spanned(
                first,
                "expected a markup path string, or parent expression followed by a path string",
            ));
        };
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
        if !input.is_empty() {
            return Err(input.error("unexpected input after markup path"));
        }
        Ok(Self { parent, file })
    }
}

/// Reads and parses the entry file and its imports.
fn sources(file: &LitStr) -> syn::Result<(std::path::PathBuf, Sources)> {
    let diagnostic = |message| syn::Error::new(file.span(), message);
    let relative = file.value();
    if Path::new(&relative).is_absolute() {
        return Err(diagnostic(
            "markup paths must be relative to CARGO_MANIFEST_DIR".into(),
        ));
    }
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR")
        .ok_or_else(|| diagnostic("CARGO_MANIFEST_DIR is unavailable".into()))?;
    let manifest = std::path::PathBuf::from(manifest);
    let limit = aegle_markup::Limits::default().max_source_bytes as u64;
    let sources = Sources::load(&relative, &mut |path| {
        let mut source = String::new();
        File::open(manifest.join(path))
            .and_then(|file| file.take(limit + 1).read_to_string(&mut source))
            .map_err(|error| error.to_string())?;
        Ok(source)
    })
    .map_err(|error| diagnostic(error.0))?;
    Ok((manifest, sources))
}

/// Splits the `$crate;` a wrapper macro puts ahead of its input. Generated
/// code names that crate by this path, which holds under any dependency rename.
fn split_path(input: Tokens) -> (Tokens, Tokens) {
    let mut trees = input.into_iter();
    let path = trees
        .by_ref()
        .take_while(|tree| !matches!(tree, TokenTree::Punct(punct) if punct.as_char() == ';'))
        .collect();
    (path, trees.collect())
}

fn expand(
    facade: &Tokens,
    arguments: &Arguments,
    specs: &[ElementSpec<'static>],
) -> syn::Result<Tokens> {
    let file = &arguments.file;
    let diagnostic = |message| syn::Error::new(file.span(), message);
    let (manifest, sources) = sources(file)?;
    let is_static = sources.is_static();
    let (program, files) = sources.check(specs).map_err(|error| diagnostic(error.0))?;
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
    let cx = generate::Context {
        loader: quote! { #facade::loader },
        facade: facade.clone(),
        program: &program,
        specs,
        span: file.span(),
    };
    let builder = if is_static {
        generate::builder(&cx)
    } else {
        dynamic::builder(&cx)
    };
    let invocation = match &arguments.parent {
        Some(parent) => quote! {
            let __aegle_parent = &(#parent);
            (#builder)(__aegle_parent)
        },
        None => builder,
    };
    Ok(quote! {{
        #(const _: &str = ::core::include_str!(#dependencies);)*
        #invocation
    }})
}
