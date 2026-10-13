use std::collections::{BTreeMap, BTreeSet};

use syn::{Expr, GenericArgument, PathArguments, Type};

use crate::analyze::model::{resolve_type_path, FileModel};

use super::control::{branch_reachability, BranchReachability};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Owner {
    Parameter(String),
    Global(String),
    Receiver,
    Private(Allocation),
    Captured(Capture),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Allocation {
    pub(super) callable: usize,
    pub(super) key: String,
    pub(super) name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Capture {
    pub(super) callable: usize,
    pub(super) name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ValueType {
    #[default]
    Unknown,
    Scalar,
    Sequence(Box<ValueType>),
    Shared(Box<ValueType>),
    Interior(Box<ValueType>),
    Wrapper(Box<ValueType>),
    Iterator(Box<ValueType>),
    Future,
    Tuple(Vec<ValueType>),
    Pointer,
    Custom(String),
}

impl ValueType {
    fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::Sequence(left), Self::Sequence(right)) => {
                Self::Sequence(Box::new(join_inner(*left, *right)))
            }
            (Self::Shared(left), Self::Shared(right)) => {
                Self::Shared(Box::new(join_inner(*left, *right)))
            }
            (Self::Interior(left), Self::Interior(right)) => {
                Self::Interior(Box::new(join_inner(*left, *right)))
            }
            (Self::Wrapper(left), Self::Wrapper(right)) => {
                Self::Wrapper(Box::new(join_inner(*left, *right)))
            }
            (left, right) if left == right => left,
            _ => Self::Unknown,
        }
    }

    pub(super) fn shared_elements(&self) -> bool {
        match self {
            Self::Shared(_) | Self::Interior(_) => true,
            Self::Sequence(inner) | Self::Wrapper(inner) | Self::Iterator(inner) => {
                inner.shared_elements()
            }
            Self::Tuple(items) => items.iter().any(Self::shared_elements),
            _ => false,
        }
    }

    pub(super) fn standard(&self) -> bool {
        !matches!(self, Self::Unknown | Self::Custom(_) | Self::Pointer)
    }

    pub(super) fn element(&self) -> Self {
        match self {
            Self::Sequence(inner)
            | Self::Shared(inner)
            | Self::Interior(inner)
            | Self::Wrapper(inner)
            | Self::Iterator(inner) => *inner.clone(),
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Binding {
    pub(super) outer: BTreeSet<Owner>,
    pub(super) elements: BTreeSet<Owner>,
    pub(super) ty: ValueType,
    pub(super) callables: BTreeMap<usize, Bindings>,
    pub(super) callback: Option<String>,
    pub(super) deferred: BTreeMap<usize, Bindings>,
    pub(super) deferred_callbacks: BTreeMap<String, Vec<Binding>>,
    pub(super) parts: Vec<Binding>,
    pub(super) unresolved: bool,
}

impl Binding {
    pub(super) fn join(mut self, other: Self) -> Self {
        if self.is_empty_fact() {
            return other;
        }
        if other.is_empty_fact() {
            return self;
        }
        self.outer.extend(other.outer);
        self.elements.extend(other.elements);
        self.ty = self.ty.join(other.ty);
        merge_invocations(&mut self.callables, other.callables);
        merge_invocations(&mut self.deferred, other.deferred);
        self.deferred_callbacks.extend(other.deferred_callbacks);
        if self.callback != other.callback {
            self.unresolved |= self.callback.is_some() || other.callback.is_some();
            self.callback = None;
        }
        self.unresolved |= other.unresolved;
        if self.parts.len() == other.parts.len() {
            self.parts = self
                .parts
                .into_iter()
                .zip(other.parts)
                .map(|(a, b)| a.join(b))
                .collect();
        } else {
            self.parts.clear();
        }
        self
    }

    fn is_empty_fact(&self) -> bool {
        self.outer.is_empty()
            && self.elements.is_empty()
            && matches!(self.ty, ValueType::Unknown)
            && self.callables.is_empty()
            && self.callback.is_none()
            && self.deferred.is_empty()
            && self.deferred_callbacks.is_empty()
            && self.parts.is_empty()
            && !self.unresolved
    }

    pub(super) fn shallow_copy(mut self) -> Self {
        if self.ty.shared_elements() {
            self.elements.extend(self.outer.iter().cloned());
        }
        if matches!(self.ty, ValueType::Sequence(_)) {
            self.outer.clear();
        }
        self
    }

    pub(super) fn indexed(mut self) -> Self {
        self.outer.extend(self.elements.iter().cloned());
        self.ty = self.ty.element();
        self
    }
}

pub(super) type Bindings = BTreeMap<String, Binding>;

fn join_inner(left: ValueType, right: ValueType) -> ValueType {
    match (left, right) {
        (ValueType::Unknown, ty) | (ty, ValueType::Unknown) => ty,
        (left, right) => left.join(right),
    }
}

pub(super) fn join_bindings(left: &mut Bindings, right: Bindings) {
    for (name, binding) in right {
        let before = left.remove(&name).unwrap_or_default();
        left.insert(name, before.join(binding));
    }
}

pub(super) fn merge_invocations(
    left: &mut BTreeMap<usize, Bindings>,
    right: BTreeMap<usize, Bindings>,
) {
    for (target, args) in right {
        join_bindings(left.entry(target).or_default(), args);
    }
}

pub(super) fn value_type(ty: &Type, model: &FileModel<'_>, scope: &str) -> ValueType {
    match ty {
        Type::Reference(reference) => value_type(&reference.elem, model, scope),
        Type::Slice(slice) => ValueType::Sequence(Box::new(value_type(&slice.elem, model, scope))),
        Type::Array(array) => ValueType::Sequence(Box::new(value_type(&array.elem, model, scope))),
        Type::Path(path) => path_type(&path.path, model, scope),
        Type::Tuple(tuple) => ValueType::Tuple(
            tuple
                .elems
                .iter()
                .map(|ty| value_type(ty, model, scope))
                .collect(),
        ),
        Type::Ptr(_) => ValueType::Pointer,
        _ => ValueType::Unknown,
    }
}

fn path_type(path: &syn::Path, model: &FileModel<'_>, scope: &str) -> ValueType {
    let Some(last) = path.segments.last() else {
        return ValueType::Unknown;
    };
    let name = last.ident.to_string();
    let key = resolve_type_path(&path_text(path), scope, &model.imports);
    if model.types.iter().any(|item| item.key == key) {
        return ValueType::Custom(key);
    }
    let relative = key.strip_prefix(&format!("{scope}::")).unwrap_or(&key);
    let resolved_name = relative.rsplit("::").next().unwrap_or(&name);
    if relative == name && crate::analyze::model::is_builtin_type(&name) && name != "Self" {
        return ValueType::Scalar;
    }
    if !standard_type_name(relative, resolved_name) {
        return ValueType::Custom(key);
    }
    let inner = type_argument(last, model, scope);
    standard_path_type(resolved_name, inner)
}

fn type_argument(segment: &syn::PathSegment, model: &FileModel<'_>, scope: &str) -> ValueType {
    match &segment.arguments {
        PathArguments::AngleBracketed(args) => args.args.iter().find_map(|arg| match arg {
            GenericArgument::Type(ty) => Some(value_type(ty, model, scope)),
            _ => None,
        }),
        _ => None,
    }
    .unwrap_or_default()
}

fn standard_type_name(key: &str, name: &str) -> bool {
    key.starts_with("std::")
        || key.starts_with("core::")
        || key.starts_with("alloc::")
        || (key == name && matches!(name, "Vec" | "String" | "Option" | "Result" | "Box"))
}

fn standard_path_type(name: &str, inner: ValueType) -> ValueType {
    match name {
        "Vec" | "String" => ValueType::Sequence(Box::new(inner)),
        "str" => ValueType::Sequence(Box::new(ValueType::Scalar)),
        "Rc" | "Arc" => ValueType::Shared(Box::new(inner)),
        "Cell" | "RefCell" | "Mutex" | "RwLock" | "AtomicUsize" | "AtomicI32" | "AtomicU32"
        | "AtomicBool" | "AtomicU64" => ValueType::Interior(Box::new(inner)),
        "Option" | "Result" | "Box" | "Ref" | "RefMut" | "MutexGuard" | "LazyLock" | "OnceLock"
        | "OnceCell" => ValueType::Wrapper(Box::new(inner)),
        _ => ValueType::Unknown,
    }
}

pub(super) fn path_text(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

pub(super) fn parameter(name: &str, ty: &Type, model: &FileModel<'_>, scope: &str) -> Binding {
    let value = value_type(ty, model, scope);
    let owner = Owner::Parameter(name.to_string());
    Binding {
        outer: if matches!(ty, Type::Reference(_))
            || matches!(value, ValueType::Shared(_) | ValueType::Interior(_))
        {
            BTreeSet::from([owner.clone()])
        } else {
            BTreeSet::new()
        },
        elements: if value.shared_elements() || borrowed_elements(ty) {
            BTreeSet::from([owner])
        } else {
            BTreeSet::new()
        },
        ty: value,
        callback: Some(name.to_string()),
        parts: match ty {
            Type::Tuple(tuple) => tuple
                .elems
                .iter()
                .map(|ty| parameter(name, ty, model, scope))
                .collect(),
            _ => Vec::new(),
        },
        ..Binding::default()
    }
}

pub(super) fn expression(
    expr: &Expr,
    bindings: &Bindings,
    model: &FileModel<'_>,
    scope: &str,
) -> Binding {
    match expr {
        Expr::Path(path) => path_binding(path, bindings, model, scope),
        Expr::Lit(_) | Expr::Binary(_) | Expr::Cast(_) => Binding {
            ty: ValueType::Scalar,
            ..Binding::default()
        },
        _ => derived_expression(expr, bindings, model, scope),
    }
}

fn derived_expression(
    expr: &Expr,
    bindings: &Bindings,
    model: &FileModel<'_>,
    scope: &str,
) -> Binding {
    if let Some(binding) = wrapped_expression(expr, bindings, model, scope) {
        return binding;
    }
    if let Some(binding) = value_expression(expr, bindings, model, scope) {
        return binding;
    }
    Binding::default()
}

fn wrapped_expression(
    expr: &Expr,
    bindings: &Bindings,
    model: &FileModel<'_>,
    scope: &str,
) -> Option<Binding> {
    match expr {
        Expr::Reference(reference) => Some(expression(&reference.expr, bindings, model, scope)),
        Expr::Paren(paren) => Some(expression(&paren.expr, bindings, model, scope)),
        Expr::Group(group) => Some(expression(&group.expr, bindings, model, scope)),
        Expr::Unary(unary) => Some(expression(&unary.expr, bindings, model, scope)),
        Expr::Index(index) => Some(expression(&index.expr, bindings, model, scope).indexed()),
        Expr::Try(node) => Some(expression(&node.expr, bindings, model, scope).indexed()),
        _ => None,
    }
}

fn value_expression(
    expr: &Expr,
    bindings: &Bindings,
    model: &FileModel<'_>,
    scope: &str,
) -> Option<Binding> {
    match expr {
        Expr::Field(field) => Some(field_binding(field, bindings, model, scope)),
        Expr::MethodCall(call) => Some(method_binding(call, bindings, model, scope)),
        Expr::Call(call) => Some(constructor_binding(call, bindings, model, scope)),
        Expr::Block(block) => Some(block_value(&block.block, bindings, model, scope)),
        Expr::If(branch) => Some(if_binding(branch, bindings, model, scope)),
        Expr::Match(node) => Some(node.arms.iter().fold(Binding::default(), |value, arm| {
            value.join(expression(&arm.body, bindings, model, scope))
        })),
        _ => aggregate_expression(expr, bindings, model, scope),
    }
}

fn aggregate_expression(
    expr: &Expr,
    bindings: &Bindings,
    model: &FileModel<'_>,
    scope: &str,
) -> Option<Binding> {
    match expr {
        Expr::Array(array) => Some(Binding {
            ty: ValueType::Sequence(Box::new(
                array
                    .elems
                    .first()
                    .map(|item| expression(item, bindings, model, scope).ty)
                    .unwrap_or_default(),
            )),
            ..Binding::default()
        }),
        Expr::Tuple(tuple) => Some(tuple_binding(tuple, bindings, model, scope)),
        Expr::Struct(node) => Some(Binding {
            ty: path_type(&node.path, model, scope),
            ..Binding::default()
        }),
        Expr::Macro(node) if node.mac.path.is_ident("vec") || node.mac.path.is_ident("format") => {
            Some(Binding {
                ty: ValueType::Sequence(Box::new(ValueType::Scalar)),
                ..Binding::default()
            })
        }
        Expr::Range(_) => Some(Binding {
            ty: ValueType::Iterator(Box::new(ValueType::Scalar)),
            ..Binding::default()
        }),
        _ => None,
    }
}

fn if_binding(
    branch: &syn::ExprIf,
    bindings: &Bindings,
    model: &FileModel<'_>,
    scope: &str,
) -> Binding {
    match branch_reachability(&branch.cond) {
        BranchReachability::ThenOnly => block_value(&branch.then_branch, bindings, model, scope),
        BranchReachability::ElseOnly => branch
            .else_branch
            .as_ref()
            .map(|(_, expr)| expression(expr, bindings, model, scope))
            .unwrap_or_default(),
        BranchReachability::Both => block_value(&branch.then_branch, bindings, model, scope).join(
            branch
                .else_branch
                .as_ref()
                .map(|(_, expr)| expression(expr, bindings, model, scope))
                .unwrap_or_default(),
        ),
    }
}

fn tuple_binding(
    tuple: &syn::ExprTuple,
    bindings: &Bindings,
    model: &FileModel<'_>,
    scope: &str,
) -> Binding {
    let parts: Vec<_> = tuple
        .elems
        .iter()
        .map(|expr| expression(expr, bindings, model, scope))
        .collect();
    Binding {
        ty: ValueType::Tuple(parts.iter().map(|part| part.ty.clone()).collect()),
        parts,
        ..Binding::default()
    }
}

fn path_binding(
    path: &syn::ExprPath,
    bindings: &Bindings,
    model: &FileModel<'_>,
    scope: &str,
) -> Binding {
    let name = path
        .path
        .segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_default();
    if let Some(binding) = bindings.get(&name) {
        return binding.clone();
    }
    if model.shared_statics.contains(&name) {
        return Binding {
            outer: BTreeSet::from([Owner::Global(name.clone())]),
            ty: model
                .static_types
                .get(&name)
                .map(|ty| value_type(ty, model, scope))
                .unwrap_or(ValueType::Interior(Box::default())),
            ..Binding::default()
        };
    }
    if model.constants.iter().any(|item| item.name == name) {
        return Binding {
            ty: model
                .static_types
                .get(&name)
                .map(|ty| value_type(ty, model, scope))
                .unwrap_or(ValueType::Scalar),
            ..Binding::default()
        };
    }
    if let Some(ty) = model.types.iter().find(|ty| ty.name == name) {
        return Binding {
            ty: ValueType::Custom(ty.key.clone()),
            ..Binding::default()
        };
    }
    Binding::default()
}

fn field_binding(
    field: &syn::ExprField,
    bindings: &Bindings,
    model: &FileModel<'_>,
    scope: &str,
) -> Binding {
    let mut binding = expression(&field.base, bindings, model, scope);
    if let ValueType::Custom(key) = &binding.ty {
        let name = match &field.member {
            syn::Member::Named(name) => name.to_string(),
            syn::Member::Unnamed(index) => index.index.to_string(),
        };
        if let Some((index, field)) = model.types.iter().find(|item| &item.key == key)
            .and_then(|item| item.fields.iter().enumerate().find(|(_, field)| field.name == name)) {
            if let Some(part) = binding.parts.get(index) {
                if !part.is_empty_fact() {
                    return part.clone();
                }
            }
            binding.ty = value_type(&field.ty, model, scope);
        } else {
            binding.ty = ValueType::Unknown;
        }
    } else if let (ValueType::Tuple(types), syn::Member::Unnamed(index)) =
        (&binding.ty, &field.member)
    {
        if let Some(part) = binding.parts.get(index.index as usize) {
            return part.clone();
        }
        binding.ty = types.get(index.index as usize).cloned().unwrap_or_default();
    } else {
        binding.ty = ValueType::Unknown;
    }
    binding
}

fn method_binding(
    call: &syn::ExprMethodCall,
    bindings: &Bindings,
    model: &FileModel<'_>,
    scope: &str,
) -> Binding {
    method_value(
        &call.method.to_string(),
        expression(&call.receiver, bindings, model, scope),
    )
}

pub(super) fn method_value(method: &str, mut receiver: Binding) -> Binding {
    match method {
        "to_vec" | "to_owned" | "to_string" | "clone" if receiver.ty.standard() => {
            receiver.shallow_copy()
        }
        "iter" | "iter_mut" => {
            receiver.ty = ValueType::Iterator(Box::new(receiver.ty.element()));
            receiver
        }
        "into_iter" | "copied" | "cloned" | "filter" | "map"
            if matches!(receiver.ty, ValueType::Iterator(_)) =>
        {
            receiver
        }
        "len" | "is_empty" | "sum" | "count" | "get" | "load" | "any" | "all" | "is_some"
        | "is_none" | "cmp" => Binding {
            ty: ValueType::Scalar,
            ..Binding::default()
        },
        _ => borrowed_method_value(method, receiver),
    }
}

fn borrowed_method_value(method: &str, mut receiver: Binding) -> Binding {
    match method {
        "borrow" | "borrow_mut" | "lock" | "try_lock" | "read" | "write" => {
            receiver.ty = receiver.ty.element();
            receiver
        }
        "ok" | "as_slice" | "as_mut_slice" => receiver,
        "unwrap" | "expect" | "as_ref" | "as_mut" => {
            if matches!(receiver.ty, ValueType::Wrapper(_)) {
                receiver.ty = receiver.ty.element();
            }
            receiver
        }
        "get_mut" | "last_mut" | "first_mut" => {
            receiver = receiver.indexed();
            receiver.ty = ValueType::Wrapper(Box::new(receiver.ty));
            receiver
        }
        _ => Binding::default(),
    }
}

fn constructor_binding(
    call: &syn::ExprCall,
    bindings: &Bindings,
    model: &FileModel<'_>,
    scope: &str,
) -> Binding {
    let Expr::Path(path) = &*call.func else {
        return Binding::default();
    };
    let key = super::resolved_call_key(path, model, scope);
    let argument = call
        .args
        .first()
        .map(|arg| expression(arg, bindings, model, scope))
        .unwrap_or_default();
    if let Some(result) = standard_return_value(&key, argument.clone()) {
        return result;
    }
    super::receiver_type(path)
        .map(|ty| constructed_value(value_type(&ty, model, scope), argument))
        .unwrap_or_default()
}

fn standard_return_value(key: &str, argument: Binding) -> Option<Binding> {
    if matches!(key, "Some" | "Ok" | "Err") {
        let mut result = argument;
        result.ty = ValueType::Wrapper(Box::new(result.ty));
        return Some(result);
    }
    if matches!(key, "Rc::clone" | "Arc::clone") {
        return Some(argument);
    }
    if super::INPUT_CALLS.contains(&key) || super::OUTPUT_CALLS.contains(&key) {
        let inner = if matches!(key, "env::var" | "fs::read_to_string") {
            ValueType::Sequence(Box::new(ValueType::Scalar))
        } else {
            ValueType::Scalar
        };
        return Some(Binding {
            ty: ValueType::Wrapper(Box::new(inner)),
            ..Binding::default()
        });
    }
    None
}

fn constructed_value(mut value: ValueType, argument: Binding) -> Binding {
    match &mut value {
        ValueType::Shared(inner) | ValueType::Interior(inner) | ValueType::Wrapper(inner)
            if matches!(**inner, ValueType::Unknown) =>
        {
            *inner = Box::new(argument.ty)
        }
        _ => {}
    }
    Binding {
        ty: value,
        elements: argument
            .outer
            .into_iter()
            .chain(argument.elements)
            .collect(),
        ..Binding::default()
    }
}

fn block_value(
    block: &syn::Block,
    bindings: &Bindings,
    model: &FileModel<'_>,
    scope: &str,
) -> Binding {
    match block.stmts.last() {
        Some(syn::Stmt::Expr(expr, None)) => expression(expr, bindings, model, scope),
        _ => Binding::default(),
    }
}

fn borrowed_elements(ty: &Type) -> bool {
    match ty {
        Type::Reference(_) => true,
        Type::Path(path) => {
            path.path
                .segments
                .iter()
                .any(|segment| match &segment.arguments {
                    PathArguments::AngleBracketed(args) => args.args.iter().any(
                        |arg| matches!(arg, GenericArgument::Type(ty) if borrowed_elements(ty)),
                    ),
                    _ => false,
                })
        }
        Type::Tuple(tuple) => tuple.elems.iter().any(borrowed_elements),
        _ => false,
    }
}
