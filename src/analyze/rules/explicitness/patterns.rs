use syn::{Pat, Type};

use crate::analyze::model::FileModel;

use super::storage::{self, Binding, Bindings, Owner, ValueType};

pub(super) fn name(pat: &Pat, position: usize) -> String {
    match pat {
        Pat::Ident(ident) => ident.ident.to_string(),
        Pat::Type(typed) => name(&typed.pat, position),
        _ => format!("$arg{position}"),
    }
}

pub(super) fn names(pat: &Pat) -> Vec<String> {
    let mut result = Vec::new();
    let mut bindings = Bindings::new();
    bind(pat, Binding::default(), &mut bindings, None);
    result.extend(bindings.into_keys());
    result
}

pub(super) fn bind(
    pat: &Pat,
    binding: Binding,
    into: &mut Bindings,
    context: Option<(&FileModel<'_>, &str)>,
) {
    match pat {
        Pat::Ident(ident) => {
            into.insert(ident.ident.to_string(), binding);
        }
        Pat::Type(typed) => {
            let mut value = binding;
            if let Some((model, scope)) = context {
                value.ty = storage::value_type(&typed.ty, model, scope);
            }
            bind(&typed.pat, value, into, context);
        }
        Pat::Reference(reference) => bind(&reference.pat, binding, into, context),
        Pat::Paren(paren) => bind(&paren.pat, binding, into, context),
        Pat::Tuple(tuple) => bind_tuple(tuple.elems.iter(), binding, into, context),
        _ => bind_collection(pat, binding, into, context),
    }
}

fn bind_collection(
    pat: &Pat,
    binding: Binding,
    into: &mut Bindings,
    context: Option<(&FileModel<'_>, &str)>,
) {
    match pat {
        Pat::TupleStruct(tuple) => {
            if matches!(binding.ty, ValueType::Wrapper(_)) {
                let value = binding.indexed();
                for pat in &tuple.elems {
                    bind(pat, value.clone(), into, context);
                }
            } else {
                bind_tuple(tuple.elems.iter(), binding, into, context);
            }
        }
        Pat::Struct(record) => bind_fields(record, binding, into, context),
        Pat::Slice(slice) => {
            for pat in &slice.elems {
                bind(pat, binding.clone().indexed(), into, context);
            }
        }
        Pat::Or(alternatives) => {
            for pat in &alternatives.cases {
                let mut branch = Bindings::new();
                bind(pat, binding.clone(), &mut branch, context);
                storage::join_bindings(into, branch);
            }
        }
        _ => {}
    }
}

fn bind_tuple<'a>(
    patterns: impl Iterator<Item = &'a Pat>,
    binding: Binding,
    into: &mut Bindings,
    context: Option<(&FileModel<'_>, &str)>,
) {
    for (index, pat) in patterns.enumerate() {
        let value = binding.parts.get(index).cloned().unwrap_or_else(|| {
            let mut value = binding.clone();
            value.ty = match &binding.ty {
                ValueType::Tuple(types) => types.get(index).cloned().unwrap_or_default(),
                _ => ValueType::Unknown,
            };
            value
        });
        bind(pat, value, into, context);
    }
}

fn bind_fields(
    record: &syn::PatStruct,
    binding: Binding,
    into: &mut Bindings,
    context: Option<(&FileModel<'_>, &str)>,
) {
    for field in &record.fields {
        let mut value = binding.clone();
        if let Some((model, scope)) = context {
            let key = match &binding.ty {
                ValueType::Custom(key) => key.clone(),
                _ => storage::path_text(&record.path),
            };
            if let Some(field_type) = model.types.iter().find(|item| item.key == key)
                .and_then(|item| item.fields.iter().find(|item| matches!(&field.member, syn::Member::Named(name) if name == item.name.as_str()))) {
                value.ty = storage::value_type(&field_type.ty, model, scope);
                if matches!(field_type.ty, Type::Reference(_)) && value.outer.is_empty() {
                    if let Some(name) = &binding.callback { value.outer.insert(Owner::Parameter(name.clone())); }
                }
            }
        }
        bind(&field.pat, value, into, context);
    }
}
