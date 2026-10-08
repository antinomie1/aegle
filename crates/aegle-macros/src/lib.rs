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
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Delimiter, TokenStream as Tokens, TokenTree};
use quote::quote;
use syn::{Expr, ExprLit, Ident, Lit, LitStr, Token, parse::Parse, parse::ParseStream};

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
/// Element names resolve in Rust scope where `ui!` is called, like types:
/// `use aegle::prelude::*` brings the built-in elements, and a control
/// library's elements come with their handle types. Each element's spec
/// reaches the checker through the macro [`element!`] defines with its name,
/// so a library's elements are checked like the built-in ones.
///
/// Paths are relative to `CARGO_MANIFEST_DIR`, including explicit `../` paths.
/// The generated dependency marker makes file edits trigger recompilation.
/// Unknown properties and unsupported language constructs are rejected with
/// file, line and column diagnostics at the path argument; an element not in
/// scope is reported by rustc as a missing macro.
#[proc_macro]
pub fn ui(input: TokenStream) -> TokenStream {
    let tokens = Tokens::from(input);
    let arguments = match syn::parse2::<Arguments>(tokens.clone()) {
        Ok(arguments) => arguments,
        Err(error) => return error.into_compile_error().into(),
    };
    let start = || -> syn::Result<Tokens> {
        let (_, sources) = sources(&arguments.file)?;
        let names = sources.elements();
        let span = arguments.file.span();
        let names: Vec<Ident> = names.iter().map(|name| Ident::new(name, span)).collect();
        resume(&[], &names, tokens, &arguments)
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
    let mut arguments = Tokens::new();
    for tree in Tokens::from(input) {
        let TokenTree::Group(group) = tree else {
            unreachable!("ui! state is grouped")
        };
        match group.delimiter() {
            Delimiter::Brace => specs.push(group.stream()),
            Delimiter::Bracket => names = group.stream().into_iter().collect(),
            _ => arguments = group.stream(),
        }
    }
    let step = || -> syn::Result<Tokens> {
        let parsed = syn::parse2::<Arguments>(arguments.clone())?;
        let names: Vec<Ident> = names
            .into_iter()
            .map(|name| syn::parse2(name.into()))
            .collect::<syn::Result<_>>()?;
        resume(&specs, &names, arguments, &parsed)
    };
    step().unwrap_or_else(syn::Error::into_compile_error).into()
}

/// Asks the next unresolved element for its spec, or expands once every
/// spec has arrived.
fn resume(
    specs: &[Tokens],
    names: &[Ident],
    arguments: Tokens,
    parsed: &Arguments,
) -> syn::Result<Tokens> {
    let facade = facade(&parsed.file)?;
    if let Some((next, rest)) = names.split_first() {
        return Ok(quote! {
            #next! { [#facade::__ui_resume] #({#specs})* [#(#rest)*] (#arguments) }
        });
    }
    let specs = specs
        .iter()
        .map(|spec| element::parse_spec(spec.clone()))
        .collect::<syn::Result<Vec<_>>>()?;
    expand(parsed, &specs)
}

/// Declares markup elements: each one's spec, and the glue that creates its
/// control and applies its properties, events and `self` fields.
///
/// ```ignore
/// aegle::element! {
///     /// A selectable chip.
///     pub Chip {
///         style text interactive pressed;
///         create |parent, text: line = ""| Chip::new(parent, text);
///         set text: line => |chip, text| chip.set_text(text);
///         set selected: bool => |chip, on| chip.set_selected(on);
///         event changed => |chip, run| chip.on_change(move |_| run());
///         get selected: bool => |chip| chip.is_selected();
///     }
/// }
/// ```
///
/// `pub Name { ... }` implements `Element` for the handle type `Name`;
/// `pub Name(Handle) { ... }` defines a marker type `Name` for elements
/// sharing a handle type. Either way it defines a hidden macro `Name` that
/// `ui!` asks for the spec: re-export the handle or marker type at the
/// crate root, where the macro lives, so one `use` brings both.
///
/// Items, each ending with `;`:
/// - `layout leaf | box | flex | grid` (default `leaf`), and for containers
///   `children any | only Name | exactly N` (default `any`);
/// - `parent Name`: it may only be written directly inside `Name`;
/// - `style` followed by the groups `text interactive pressed indicator editor`;
/// - `create |parent, name: type = default, ...| expr` returning the handle;
///   a constructor argument without a default is required and literal;
/// - `set name: type => |handle, value| ...` for a settable, bindable property;
/// - `event name => |handle, run| ...` registering `run` as a handler;
/// - `get name: type => |handle| ...` for a `self.name` field;
/// - `children => |handle, index| container` where child `index` goes.
///
/// Types are `bool`, `int`, `int(min, max)`, `float`, `float(min)`,
/// `float(min, max)`, `fraction`, `length`, `string`, `line`, `color` and
/// `choice(a, b, ...)`, arriving as `bool`, `i64`, `f64`, `f32`, `f32`,
/// `&str`, `&str`, `Color` and `&str`.
#[proc_macro]
pub fn element(input: TokenStream) -> TokenStream {
    let defs = syn::parse_macro_input!(input as element::Defs);
    let loader = match loader() {
        Ok(loader) => loader,
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

fn expand(arguments: &Arguments, specs: &[ElementSpec<'static>]) -> syn::Result<Tokens> {
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
    let facade = facade(file)?;
    let cx = generate::Context {
        loader: quote! { #facade::loader },
        facade,
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

/// The loader as seen by an `element!` call: through the facade, or the
/// loader crate itself for the built-in elements.
fn loader() -> syn::Result<Tokens> {
    let error =
        |error: proc_macro_crate::Error| syn::Error::new(proc_macro2::Span::call_site(), error);
    let path = match crate_name("aegle") {
        Ok(FoundCrate::Name(name)) => format!("::{name}::loader"),
        Ok(FoundCrate::Itself) => "::aegle::loader".into(),
        Err(_) => match crate_name("aegle-loader").map_err(error)? {
            FoundCrate::Name(name) => format!("::{name}"),
            FoundCrate::Itself => "::aegle_loader".into(),
        },
    };
    let path = syn::parse_str::<syn::Path>(&path)?;
    Ok(quote! { #path })
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
