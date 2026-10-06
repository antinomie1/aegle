//! Compile `.aegle` files into ordinary retained control construction.
//!
//! Parsing and checking run in the compiler. A static document becomes only
//! handle constructors and property setters, without a markup parser, dynamic
//! component registry or named-node lookup table. A document with states,
//! bindings, events, `if`/`for` blocks, components or imports becomes Rust
//! code that constructs its checked program; `aegle::loader` executes it,
//! without a markup parser or checker in the binary.

mod dynamic;
mod generate;
mod layout;
mod lower;
mod motion;

use std::{fs::File, io::Read, path::Path};

use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::TokenStream as Tokens;
use quote::quote;
use syn::{Expr, ExprLit, Lit, LitStr, Token, parse::Parse, parse::ParseStream};

/// Compiles a manifest-relative `.aegle` file into a typed retained view.
///
/// `ui!("view.aegle")` produces a builder closure taking `&aegle::App` for a
/// `Window` root, or `&aegle::Container` for any other component root. The builder
/// returns `aegle::Result<View>`. `ui!(parent, "view.aegle")` invokes that builder
/// immediately and evaluates the parent expression once.
///
/// The inferred view has a public `root` handle and a public typed field for
/// every markup `id`, plus a `loader::State<T>` field for every state of a
/// dynamic document's root. Bind Rust callbacks through those fields after
/// creation; IDs inside blocks and components are not exposed. Imports resolve
/// against the importing file and are tracked for recompilation too.
/// Dropping the view keeps its retained controls alive. A construction failure
/// removes the new subtree; it never removes the parent supplied by the caller.
/// Transition properties require the facade's `motion` feature. Transitions are
/// installed after every static property, so initial construction does not animate.
///
/// Paths are relative to `CARGO_MANIFEST_DIR`, including explicit `../` paths.
/// The generated dependency marker makes file edits trigger recompilation.
/// Unknown components, properties and unsupported language constructs are
/// rejected with file, line and column diagnostics at the path argument.
#[proc_macro]
pub fn ui(input: TokenStream) -> TokenStream {
    let arguments = syn::parse_macro_input!(input as Arguments);
    expand(arguments)
        .unwrap_or_else(syn::Error::into_compile_error)
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

fn expand(arguments: Arguments) -> syn::Result<Tokens> {
    let file = &arguments.file;
    let diagnostic = |message| syn::Error::new(file.span(), message);
    let relative = file.value();
    if Path::new(&relative).is_absolute() {
        return Err(diagnostic(
            "markup paths must be relative to CARGO_MANIFEST_DIR".into(),
        ));
    }
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR")
        .ok_or_else(|| diagnostic("CARGO_MANIFEST_DIR is unavailable".into()))?;
    let path = Path::new(&manifest).join(&relative);
    let io_error = |error| diagnostic(format!("{}:1:1: {error}", path.display()));
    let limit = aegle_markup::Limits::default().max_source_bytes;
    let mut source = String::new();
    File::open(&path)
        .map_err(io_error)?
        .take(limit as u64 + 1)
        .read_to_string(&mut source)
        .map_err(io_error)?;
    let document = aegle_markup::parse(&source)
        .map_err(|error| diagnostic(error.render(&source, &relative)))?;
    let path = path
        .to_str()
        .ok_or_else(|| diagnostic("markup path is not UTF-8".into()))?;
    let dependency = LitStr::new(path, file.span());
    let facade = facade(file)?;
    let builder = if document.is_static() {
        let plan = aegle_markup::check(document)
            .map_err(|error| diagnostic(error.render(&source, &relative)))?;
        generate::builder(&plan, &facade)
    } else {
        dynamic::builder(&relative, Path::new(&manifest), file, &facade)?
    };
    let invocation = match arguments.parent {
        Some(parent) => quote! {
            let __aegle_parent = &(#parent);
            (#builder)(__aegle_parent)
        },
        None => builder,
    };
    Ok(quote! {{
        const _: &str = ::core::include_str!(#dependency);
        #invocation
    }})
}

fn facade(file: &LitStr) -> syn::Result<Tokens> {
    let name = match crate_name("aegle").map_err(|error| syn::Error::new(file.span(), error))? {
        FoundCrate::Name(name) => name,
        // Also covers examples and integration tests in the facade package.
        // The facade aliases itself under this name for its own macro calls.
        FoundCrate::Itself => "aegle".into(),
    };
    let path = syn::parse_str::<syn::Path>(&format!("::{name}"))
        .or_else(|_| syn::parse_str(&format!("::r#{name}")))
        .map_err(|error| syn::Error::new(file.span(), error))?;
    Ok(quote! { #path })
}
