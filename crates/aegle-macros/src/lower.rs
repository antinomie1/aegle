//! Emits a checked program as Rust construction code, so compiled views carry
//! no markup parser or checker.

use aegle_markup::{
    Bound, Child, Element, ElementKind, Expr, ExprKind, Program, Ref, Span, Step, Template, Type,
    Value,
};
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};

/// `m` is the path of the re-exported markup crate.
pub(super) fn program(program: &Program, m: &TokenStream) -> TokenStream {
    let templates = program.templates.iter().map(|t| template(t, m));
    let ids = program.ids.iter().map(|(name, kind)| {
        let kind = variant(kind);
        quote! { (::std::string::String::from(#name), #m::Kind::#kind) }
    });
    quote! { #m::Program { templates: vec![#(#templates),*], ids: vec![#(#ids),*] } }
}

fn template(template: &Template, m: &TokenStream) -> TokenStream {
    let name = &template.name;
    let params = template.params.iter().map(|(name, ty, default)| {
        let (ty, default) = (
            self::ty(ty, m),
            option(default.as_ref().map(|e| expr(e, m))),
        );
        quote! { (::std::string::String::from(#name), #ty, #default) }
    });
    let states = template.states.iter().map(|(name, ty, value)| {
        let (ty, value) = (self::ty(ty, m), expr(value, m));
        quote! { (::std::string::String::from(#name), #ty, #value) }
    });
    let root = element(&template.root, m);
    quote! {
        #m::Template {
            name: ::std::string::String::from(#name),
            params: vec![#(#params),*],
            states: vec![#(#states),*],
            root: #root,
        }
    }
}

fn element(element: &Element, m: &TokenStream) -> TokenStream {
    let kind = match element.kind {
        ElementKind::Builtin(kind) => {
            let kind = variant(&kind);
            quote! { #m::ElementKind::Builtin(#m::Kind::#kind) }
        }
        ElementKind::Component(index) => quote! { #m::ElementKind::Component(#index) },
    };
    let id = option(element.id.map(|id| quote! { #id }));
    let properties = element.properties.iter().map(|(name, bound)| {
        let name = variant(name);
        let bound = match bound {
            Bound::Literal(v) => {
                let v = value(v, m);
                quote! { #m::Bound::Literal(#v) }
            }
            Bound::Expr(e) => {
                let e = expr(e, m);
                quote! { #m::Bound::Expr(::std::rc::Rc::new(#e)) }
            }
        };
        quote! { (#m::PropertyName::#name, #bound) }
    });
    let arguments = element.arguments.iter().map(|argument| {
        option(argument.as_ref().map(|e| {
            let e = expr(e, m);
            quote! { ::std::rc::Rc::new(#e) }
        }))
    });
    let events = element.events.iter().map(|(event, body)| {
        let (event, body) = (variant(event), body.iter().map(|s| step(s, m)));
        quote! { (#m::EventKind::#event, ::std::rc::Rc::from(vec![#(#body),*])) }
    });
    let children = element.children.iter().map(|c| child(c, m));
    let span = span(element.span, m);
    quote! {
        #m::Element {
            kind: #kind,
            id: #id,
            properties: vec![#(#properties),*],
            arguments: vec![#(#arguments),*],
            events: vec![#(#events),*],
            children: vec![#(#children),*],
            span: #span,
        }
    }
}

fn child(child: &Child, m: &TokenStream) -> TokenStream {
    let list = |children: &[Child]| {
        let children = children.iter().map(|c| self::child(c, m));
        quote! { ::std::rc::Rc::from(vec![#(#children),*]) }
    };
    match child {
        Child::Element(e) => {
            let e = element(e, m);
            quote! { #m::Child::Element(#e) }
        }
        Child::If(condition, then, otherwise) => {
            let (condition, then, otherwise) = (expr(condition, m), list(then), list(otherwise));
            quote! { #m::Child::If(::std::rc::Rc::new(#condition), #then, #otherwise) }
        }
        Child::For(items, body) => {
            let (items, body) = (expr(items, m), list(body));
            quote! { #m::Child::For(::std::rc::Rc::new(#items), #body) }
        }
    }
}

fn step(step: &Step, m: &TokenStream) -> TokenStream {
    match step {
        Step::Assign(state, operator, value) => {
            let value = expr(value, m);
            quote! { #m::Step::Assign(#state, #operator, #value) }
        }
        Step::If(condition, then, otherwise) => {
            let condition = expr(condition, m);
            let then = then.iter().map(|s| self::step(s, m));
            let otherwise = otherwise.iter().map(|s| self::step(s, m));
            quote! { #m::Step::If(#condition, vec![#(#then),*], vec![#(#otherwise),*]) }
        }
    }
}

fn expr(expr: &Expr, m: &TokenStream) -> TokenStream {
    let boxed = |e: &Expr| {
        let e = self::expr(e, m);
        quote! { ::std::boxed::Box::new(#e) }
    };
    let kind = match &expr.kind {
        ExprKind::Literal(v) => {
            let v = value(v, m);
            quote! { #m::ExprKind::Literal(#v) }
        }
        ExprKind::SelfField(field) => {
            quote! { #m::ExprKind::SelfField(::std::string::String::from(#field)) }
        }
        ExprKind::Unary(operator, operand) => {
            let operand = boxed(operand);
            quote! { #m::ExprKind::Unary(#operator, #operand) }
        }
        ExprKind::Binary(operator, left, right) => {
            let (left, right) = (boxed(left), boxed(right));
            quote! { #m::ExprKind::Binary(#operator, #left, #right) }
        }
        ExprKind::Call(function, arguments) => {
            let arguments = arguments.iter().map(|a| self::expr(a, m));
            quote! { #m::ExprKind::Call(::std::string::String::from(#function), vec![#(#arguments),*]) }
        }
        ExprKind::List(items) => {
            let items = items.iter().map(|i| self::expr(i, m));
            quote! { #m::ExprKind::List(vec![#(#items),*]) }
        }
        ExprKind::Ref(reference) => {
            let reference = match reference {
                Ref::State(i) => quote! { State(#i) },
                Ref::Param(i) => quote! { Param(#i) },
                Ref::Item(i) => quote! { Item(#i) },
            };
            quote! { #m::ExprKind::Ref(#m::Ref::#reference) }
        }
        ExprKind::Name(_) => unreachable!("checked expressions are resolved"),
    };
    let span = span(expr.span, m);
    quote! { #m::Expr { kind: #kind, span: #span } }
}

fn value(value: &Value, m: &TokenStream) -> TokenStream {
    match value {
        Value::String(v) => quote! { #m::Value::String(::std::string::String::from(#v)) },
        Value::Bool(v) => quote! { #m::Value::Bool(#v) },
        Value::Int(v) => quote! { #m::Value::Int(#v) },
        Value::Number(v) => quote! { #m::Value::Number(#v) },
        Value::Length(v) => quote! { #m::Value::Length(#v) },
        Value::Duration(v) => quote! { #m::Value::Duration(#v) },
        Value::Color([r, g, b, a]) => quote! { #m::Value::Color([#r, #g, #b, #a]) },
        Value::Identifier(v) => quote! { #m::Value::Identifier(::std::string::String::from(#v)) },
        Value::Expr(_) => unreachable!("checked literals are not expressions"),
    }
}

fn ty(ty: &Type, m: &TokenStream) -> TokenStream {
    match ty {
        Type::Bool => quote! { #m::Type::Bool },
        Type::Int => quote! { #m::Type::Int },
        Type::Float => quote! { #m::Type::Float },
        Type::String => quote! { #m::Type::String },
        Type::List(item) => {
            let item = self::ty(item, m);
            quote! { #m::Type::List(::std::boxed::Box::new(#item)) }
        }
    }
}

fn span(span: Span, m: &TokenStream) -> TokenStream {
    let (start, end) = (span.start, span.end);
    quote! { #m::Span { start: #start, end: #end } }
}

fn option(value: Option<TokenStream>) -> TokenStream {
    match value {
        Some(value) => quote! { ::core::option::Option::Some(#value) },
        None => quote! { ::core::option::Option::None },
    }
}

/// Enum variants share their Debug names.
fn variant(value: &impl std::fmt::Debug) -> Ident {
    format_ident!("{}", format!("{value:?}"))
}
