//! `element!`: one markup element's spec and the glue that drives its control.
//!
//! The same parser reads a definition twice: in the library, to implement
//! `Element`, and inside `ui!`, from the tokens the element's spec macro
//! forwards, to check markup against the spec.

mod expand;

use aegle_markup::{Children, ElementSpec, Layout, PropertySpec, Styles, ValueType};
use proc_macro2::{Span, TokenStream};
use quote::ToTokens;
use syn::{
    Attribute, Expr, Ident, LitInt, Token, Type, Visibility, braced,
    ext::IdentExt,
    parenthesized,
    parse::{Parse, ParseStream},
};

/// Every definition of one `element!` invocation.
pub(crate) struct Defs(pub Vec<Def>);

pub(crate) struct Def {
    attrs: Vec<Attribute>,
    vis: Visibility,
    pub name: Ident,
    handle: Option<Type>,
    layout: Layout,
    children: Children<'static>,
    parent: Option<&'static str>,
    styles: Styles,
    create: (Ident, Expr),
    /// Properties in spec order, with their constructor default and setter.
    properties: Vec<Property>,
    events: Vec<(Ident, Expr)>,
    fields: Vec<(Ident, ValueType<'static>, Expr)>,
    place: Option<Expr>,
    /// The definition as written, forwarded to `ui!` by the spec macro.
    tokens: TokenStream,
}

struct Property {
    name: Ident,
    ty: ValueType<'static>,
    /// A constructor argument and its default; `Some(None)` is required.
    new: Option<Option<Expr>>,
    set: Option<Expr>,
}

impl Parse for Defs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut defs = Vec::new();
        while !input.is_empty() {
            defs.push(input.parse()?);
        }
        Ok(Self(defs))
    }
}

fn leak(text: String) -> &'static str {
    text.leak()
}

/// An identifier or keyword, such as `box`.
fn word(input: ParseStream<'_>) -> syn::Result<String> {
    Ok(input.call(Ident::parse_any)?.to_string())
}

fn number(input: ParseStream<'_>) -> syn::Result<f64> {
    let negative = input.parse::<Option<Token![-]>>()?.is_some();
    let value = if input.peek(LitInt) {
        input.parse::<LitInt>()?.base10_parse::<f64>()?
    } else {
        input.parse::<syn::LitFloat>()?.base10_parse::<f64>()?
    };
    Ok(if negative { -value } else { value })
}

fn value_type(input: ParseStream<'_>) -> syn::Result<ValueType<'static>> {
    let span = input.span();
    let name = word(input)?;
    let arguments = |input: ParseStream<'_>| -> syn::Result<Vec<f64>> {
        if !input.peek(syn::token::Paren) {
            return Ok(Vec::new());
        }
        let inner;
        parenthesized!(inner in input);
        let mut values = vec![number(&inner)?];
        while inner.parse::<Option<Token![,]>>()?.is_some() {
            values.push(number(&inner)?);
        }
        Ok(values)
    };
    Ok(match name.as_str() {
        "bool" => ValueType::Bool,
        "int" => match arguments(input)?[..] {
            [] => ValueType::Int(i64::MIN, i64::MAX),
            [min, max] => ValueType::Int(min as i64, max as i64),
            _ => return Err(syn::Error::new(span, "int takes no range or (min, max)")),
        },
        "float" => match arguments(input)?[..] {
            [] => ValueType::Float(f64::MIN, f64::MAX),
            [min] => ValueType::Float(min, f64::MAX),
            [min, max] => ValueType::Float(min, max),
            _ => return Err(syn::Error::new(span, "float takes (min) or (min, max)")),
        },
        "fraction" => ValueType::Fraction,
        "length" => ValueType::Length,
        "string" => ValueType::String,
        "line" => ValueType::Line,
        "color" => ValueType::Color,
        "choice" => {
            let inner;
            parenthesized!(inner in input);
            let mut choices = Vec::new();
            while !inner.is_empty() {
                choices.push(leak(word(&inner)?));
                inner.parse::<Option<Token![,]>>()?;
            }
            ValueType::Choice(Vec::leak(choices))
        }
        _ => {
            return Err(syn::Error::new(
                span,
                format!("unknown value type `{name}`"),
            ));
        }
    })
}

impl Parse for Def {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let tokens = input.fork().parse::<DefTokens>()?.0;
        let attrs = input.call(Attribute::parse_outer)?;
        let vis = input.parse()?;
        let name: Ident = input.parse()?;
        let handle = if input.peek(syn::token::Paren) {
            let inner;
            parenthesized!(inner in input);
            Some(inner.parse()?)
        } else {
            None
        };
        let body;
        braced!(body in input);
        let mut def = Def {
            attrs,
            vis,
            name,
            handle,
            layout: Layout::Leaf,
            children: Children::Any,
            parent: None,
            styles: Styles::NONE,
            create: (
                Ident::new("parent", Span::call_site()),
                syn::parse_quote!(()),
            ),
            properties: Vec::new(),
            events: Vec::new(),
            fields: Vec::new(),
            place: None,
            tokens,
        };
        let mut created = false;
        while !body.is_empty() {
            let span = body.span();
            let keyword = word(&body)?;
            match keyword.as_str() {
                "layout" => {
                    def.layout = match word(&body)?.as_str() {
                        "leaf" => Layout::Leaf,
                        "box" => Layout::Box,
                        "flex" => Layout::Flex,
                        "grid" => Layout::Grid,
                        _ => {
                            return Err(syn::Error::new(span, "layout is leaf, box, flex or grid"));
                        }
                    }
                }
                "children" if body.peek(Token![=>]) => {
                    body.parse::<Token![=>]>()?;
                    def.place = Some(body.parse()?);
                }
                "children" => {
                    def.children = match word(&body)?.as_str() {
                        "any" => Children::Any,
                        "only" => Children::Only(leak(word(&body)?)),
                        "exactly" => Children::Exactly(body.parse::<LitInt>()?.base10_parse()?),
                        _ => {
                            return Err(syn::Error::new(
                                span,
                                "children are any, only Name or exactly N",
                            ));
                        }
                    }
                }
                "parent" => def.parent = Some(leak(word(&body)?)),
                "style" => {
                    while !body.peek(Token![;]) {
                        let group = word(&body)?;
                        def.styles = def.styles.with(match group.as_str() {
                            "text" => Styles::TEXT,
                            "interactive" => Styles::INTERACTIVE,
                            "pressed" => Styles::PRESSED,
                            "indicator" => Styles::INDICATOR,
                            "editor" => Styles::EDITOR,
                            _ => {
                                return Err(syn::Error::new(
                                    span,
                                    format!("unknown style group `{group}`"),
                                ));
                            }
                        });
                    }
                }
                "create" => {
                    created = true;
                    body.parse::<Token![|]>()?;
                    let parent: Ident = body.parse()?;
                    while body.parse::<Option<Token![,]>>()?.is_some() {
                        let name: Ident = body.parse()?;
                        body.parse::<Token![:]>()?;
                        let ty = value_type(&body)?;
                        let default = if body.parse::<Option<Token![=]>>()?.is_some() {
                            let negative = body.parse::<Option<Token![-]>>()?;
                            let literal: syn::Lit = body.parse()?;
                            Some(syn::parse_quote!(#negative #literal))
                        } else {
                            None
                        };
                        def.properties.push(Property {
                            name,
                            ty,
                            new: Some(default),
                            set: None,
                        });
                    }
                    body.parse::<Token![|]>()?;
                    def.create = (parent, body.parse()?);
                }
                "set" => {
                    let name: Ident = body.parse()?;
                    body.parse::<Token![:]>()?;
                    let ty = value_type(&body)?;
                    body.parse::<Token![=>]>()?;
                    let setter = body.parse()?;
                    match def.properties.iter_mut().find(|p| p.name == name) {
                        Some(property) if property.ty == ty => property.set = Some(setter),
                        Some(_) => {
                            return Err(syn::Error::new(
                                name.span(),
                                "the setter's type differs from the constructor's",
                            ));
                        }
                        None => def.properties.push(Property {
                            name,
                            ty,
                            new: None,
                            set: Some(setter),
                        }),
                    }
                }
                "event" => {
                    let name = body.parse()?;
                    body.parse::<Token![=>]>()?;
                    def.events.push((name, body.parse()?));
                }
                "get" => {
                    let name = body.parse()?;
                    body.parse::<Token![:]>()?;
                    let ty = value_type(&body)?;
                    body.parse::<Token![=>]>()?;
                    def.fields.push((name, ty, body.parse()?));
                }
                _ => {
                    return Err(syn::Error::new(
                        span,
                        format!("unknown element item `{keyword}`"),
                    ));
                }
            }
            body.parse::<Token![;]>()?;
        }
        if !created {
            return Err(syn::Error::new(
                def.name.span(),
                "an element needs `create |parent, ...| ...;`",
            ));
        }
        Ok(def)
    }
}

/// The tokens of one definition: attributes, visibility, name, optional
/// handle type and body.
struct DefTokens(TokenStream);

impl Parse for DefTokens {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut tokens = TokenStream::new();
        for attr in input.call(Attribute::parse_outer)? {
            attr.to_tokens(&mut tokens);
        }
        input.parse::<Visibility>()?.to_tokens(&mut tokens);
        input.parse::<Ident>()?.to_tokens(&mut tokens);
        if input.peek(syn::token::Paren) {
            input.parse::<proc_macro2::Group>()?.to_tokens(&mut tokens);
        }
        input.parse::<proc_macro2::Group>()?.to_tokens(&mut tokens);
        Ok(Self(tokens))
    }
}

impl Def {
    /// The spec markup is checked against.
    pub(crate) fn spec(&self) -> ElementSpec<'static> {
        let properties = self.properties.iter().map(|property| PropertySpec {
            name: leak(property.name.to_string()),
            ty: property.ty,
            new: property.new.is_some(),
            set: property.set.is_some(),
            required: matches!(property.new, Some(None)),
        });
        let events = self.events.iter().map(|(name, _)| leak(name.to_string()));
        let fields = self
            .fields
            .iter()
            .map(|(name, ty, _)| (leak(name.to_string()), *ty));
        ElementSpec {
            name: leak(self.name.to_string()),
            layout: self.layout,
            children: self.children,
            parent: self.parent,
            styles: self.styles,
            properties: Vec::leak(properties.collect()),
            events: Vec::leak(events.collect()),
            fields: Vec::leak(fields.collect()),
        }
    }
}

/// Reads a forwarded definition back as a spec inside `ui!`.
pub(crate) fn parse_spec(tokens: TokenStream) -> syn::Result<ElementSpec<'static>> {
    Ok(syn::parse2::<Def>(tokens)?.spec())
}
