//! Emits a checked program as Rust construction code, so compiled views carry
//! no markup parser or checker.

use aegle_markup::{
    Bound, Child, Element, ElementKind, Expr, ExprKind, Program, Prop, Ref, Span, Step, Template,
    Type, Value,
};
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};

/// `m` is the path of the re-exported markup crate.
pub(super) fn program(program: &Program, m: &TokenStream) -> TokenStream {
    let templates = program.templates.iter().map(|t| template(t, m));
    let ids = program.ids.iter().map(|(name, kind)| {
        let kind = element_kind(*kind, m);
        quote! { (::std::string::String::from(#name), #kind) }
    });
    let elements = program.elements.iter();
    let records = program.records.iter().map(|record| {
        let name = &record.name;
        let fields = record.fields.iter().map(|(field, ty)| {
            let ty = self::ty(ty, m);
            quote! { (::std::string::String::from(#field), #ty) }
        });
        let span = span(record.span, m);
        quote! {
            #m::Record {
                name: ::std::string::String::from(#name),
                fields: vec![#(#fields),*],
                span: #span,
            }
        }
    });
    let host_calls = program.host_calls.iter().map(|call| {
        let (name, file) = (&call.name, call.file);
        let types = call.types.iter().map(|t| self::ty(t, m));
        let span = span(call.span, m);
        quote! {
            #m::HostCall {
                name: ::std::string::String::from(#name),
                types: vec![#(#types),*],
                file: #file,
                span: #span,
            }
        }
    });
    let files = program
        .files
        .iter()
        .map(|file| quote! { ::std::string::String::from(#file) });
    quote! {
        #m::Program {
            templates: vec![#(#templates),*],
            elements: vec![#(::std::string::String::from(#elements)),*],
            ids: vec![#(#ids),*],
            records: vec![#(#records),*],
            host_calls: vec![#(#host_calls),*],
            files: vec![#(#files),*],
        }
    }
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
    let events = template.events.iter().map(|(name, ty)| {
        let ty = option(ty.as_ref().map(|t| self::ty(t, m)));
        quote! { (::std::string::String::from(#name), #ty) }
    });
    let (slot, file) = (template.slot, template.file);
    let root = element(&template.root, m);
    quote! {
        #m::Template {
            name: ::std::string::String::from(#name),
            params: vec![#(#params),*],
            states: vec![#(#states),*],
            events: vec![#(#events),*],
            slot: #slot,
            file: #file,
            root: #root,
        }
    }
}

fn element_kind(kind: ElementKind, m: &TokenStream) -> TokenStream {
    match kind {
        ElementKind::Window => quote! { #m::ElementKind::Window },
        ElementKind::Control(index) => quote! { #m::ElementKind::Control(#index) },
        ElementKind::Component(index) => quote! { #m::ElementKind::Component(#index) },
    }
}

fn element(element: &Element, m: &TokenStream) -> TokenStream {
    let kind = element_kind(element.kind, m);
    let id = option(element.id.map(|id| quote! { #id }));
    let properties = element.properties.iter().map(|(prop, bound)| {
        let prop = match prop {
            Prop::Node(name) => {
                let name = variant(name);
                quote! { #m::Prop::Node(#m::PropertyName::#name) }
            }
            Prop::Element(index) => quote! { #m::Prop::Element(#index) },
        };
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
        quote! { (#prop, #bound) }
    });
    let arguments = element.arguments.iter().map(|argument| {
        option(argument.as_ref().map(|e| {
            let e = expr(e, m);
            quote! { ::std::rc::Rc::new(#e) }
        }))
    });
    let events = element.events.iter().map(|(event, body)| {
        let body = body.iter().map(|s| step(s, m));
        quote! { (#event, ::std::rc::Rc::from(vec![#(#body),*])) }
    });
    let handlers = element.handlers.iter().map(|handler| {
        let (event, binds_value) = (handler.event, handler.binds_value);
        let steps = handler.steps.iter().map(|s| step(s, m));
        quote! {
            #m::Handler {
                event: #event,
                binds_value: #binds_value,
                steps: ::std::rc::Rc::from(vec![#(#steps),*]),
            }
        }
    });
    let slot = option(element.slot.as_ref().map(|children| {
        let children = children.iter().map(|c| child(c, m));
        quote! { ::std::rc::Rc::from(vec![#(#children),*]) }
    }));
    let children = element.children.iter().map(|c| child(c, m));
    let span = span(element.span, m);
    quote! {
        #m::Element {
            kind: #kind,
            id: #id,
            properties: vec![#(#properties),*],
            arguments: vec![#(#arguments),*],
            events: vec![#(#events),*],
            handlers: vec![#(#handlers),*],
            children: vec![#(#children),*],
            slot: #slot,
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
        Child::For(items, key, body) => {
            let (items, body) = (expr(items, m), list(body));
            let key = option(key.as_ref().map(|key| {
                let key = expr(key, m);
                quote! { ::std::rc::Rc::new(#key) }
            }));
            quote! { #m::Child::For(::std::rc::Rc::new(#items), #key, #body) }
        }
        Child::Slot => quote! { #m::Child::Slot },
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
        Step::Let(value) => {
            let value = expr(value, m);
            quote! { #m::Step::Let(#value) }
        }
        Step::Host(name, arguments, at) => {
            let arguments = arguments.iter().map(|a| expr(a, m));
            let at = span(*at, m);
            quote! {
                #m::Step::Host(::std::string::String::from(#name), vec![#(#arguments),*], #at)
            }
        }
        Step::Emit(event, value) => {
            let value = option(value.as_ref().map(|v| expr(v, m)));
            quote! { #m::Step::Emit(#event, #value) }
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
        ExprKind::Record(index, values) => {
            let values = values.iter().map(|v| self::expr(v, m));
            quote! { #m::ExprKind::Record(#index, vec![#(#values),*]) }
        }
        ExprKind::FieldAt(record, index) => {
            let record = boxed(record);
            quote! { #m::ExprKind::FieldAt(#record, #index) }
        }
        ExprKind::Ref(reference) => {
            let reference = match reference {
                Ref::State(i) => quote! { State(#i) },
                Ref::Param(i) => quote! { Param(#i) },
                Ref::Item(i) => quote! { Item(#i) },
                Ref::Local(i) => quote! { Local(#i) },
            };
            quote! { #m::ExprKind::Ref(#m::Ref::#reference) }
        }
        ExprKind::Name(_) | ExprKind::Field(..) => {
            unreachable!("checked expressions are resolved")
        }
    };
    let span = span(expr.span, m);
    quote! { #m::Expr { kind: #kind, span: #span } }
}

pub(crate) fn value(literal: &Value, m: &TokenStream) -> TokenStream {
    match literal {
        Value::String(v) => quote! { #m::Value::String(::std::string::String::from(#v)) },
        Value::Bool(v) => quote! { #m::Value::Bool(#v) },
        Value::Int(v) => quote! { #m::Value::Int(#v) },
        Value::Number(v) => quote! { #m::Value::Number(#v) },
        Value::Length(v) => quote! { #m::Value::Length(#v) },
        Value::Percent(v) => quote! { #m::Value::Percent(#v) },
        Value::Fraction(v) => quote! { #m::Value::Fraction(#v) },
        Value::List(items) => {
            let items = items.iter().map(|item| value(item, m));
            quote! { #m::Value::List(::std::vec![#(#items),*]) }
        }
        Value::Call(name, arguments) => {
            let arguments = arguments.iter().map(|argument| value(argument, m));
            quote! {
                #m::Value::Call(::std::string::String::from(#name), ::std::vec![#(#arguments),*])
            }
        }
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
        Type::Record(name) => quote! { #m::Type::Record(::std::string::String::from(#name)) },
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
pub(crate) fn variant(value: &impl std::fmt::Debug) -> Ident {
    format_ident!("{}", format!("{value:?}"))
}
