//! AST walkers that build the file model.

use std::collections::{HashMap, HashSet};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Fields, FnArg, Item, ItemEnum, ItemFn, ItemImpl, ItemStruct, ItemTrait, ItemUnion, Member,
    ReturnType, UseTree,
};

use crate::analyze::helpers::is_public;
use crate::analyze::is_test_module;

use super::use_def::is_binding_name;
use super::{
    bool_params, count_params, field_stats, full_type_path_from_type, returns_bool,
    type_name_from_path, DuplicateKey, FieldInfo, FnModel, MethodRef, NamedBinding, StaticMutSite,
    TypeModel,
};

#[derive(Default)]
pub(crate) struct DuplicateKeyCollector {
    pub(crate) keys: Vec<DuplicateKey>,
}


impl<'ast> Visit<'ast> for DuplicateKeyCollector {
    fn visit_expr_struct(&mut self, node: &'ast syn::ExprStruct) {
        let mut seen: HashMap<String, usize> = HashMap::new();
        for field in &node.fields {
            let Member::Named(ident) = &field.member else {
                continue;
            };
            let name = ident.to_string();
            let line = ident.span().start().line;
            if let Some(first) = seen.get(&name) {
                self.keys.push(DuplicateKey {
                    display: name,
                    line,
                    first_line: *first,
                });
            } else {
                seen.insert(name, line);
            }
        }
        syn::visit::visit_expr_struct(self, node);
    }
}


pub(crate) struct BindingCollector {
    pub(crate) variables: Vec<NamedBinding>,
    pub(crate) constants: Vec<NamedBinding>,
}


impl<'ast> Visit<'ast> for BindingCollector {
    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        // Skip the loop pattern: naming rules treat binders as out of scope, which
        // matches not recording them at all.
        self.visit_expr(&node.expr);
        self.visit_block(&node.body);
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        if let syn::Expr::Let(l) = &*node.cond {
            self.visit_expr(&l.expr);
        } else {
            self.visit_expr(&node.cond);
        }
        self.visit_block(&node.body);
    }

    fn visit_pat_ident(&mut self, node: &'ast syn::PatIdent) {
        if is_binding_name(&node.ident.to_string()) {
            self.variables.push(NamedBinding {
                name: node.ident.to_string(),
                begin_line: node.ident.span().start().line,
            });
        }
    }

    fn visit_field(&mut self, node: &'ast syn::Field) {
        if let Some(ident) = &node.ident {
            self.variables.push(NamedBinding {
                name: ident.to_string(),
                begin_line: ident.span().start().line,
            });
        }
    }

    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        self.constants.push(NamedBinding {
            name: node.ident.to_string(),
            begin_line: node.ident.span().start().line,
        });
    }

    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        self.constants.push(NamedBinding {
            name: node.ident.to_string(),
            begin_line: node.ident.span().start().line,
        });
    }

    fn visit_impl_item_const(&mut self, node: &'ast syn::ImplItemConst) {
        self.constants.push(NamedBinding {
            name: node.ident.to_string(),
            begin_line: node.ident.span().start().line,
        });
    }

    fn visit_trait_item_const(&mut self, node: &'ast syn::TraitItemConst) {
        self.constants.push(NamedBinding {
            name: node.ident.to_string(),
            begin_line: node.ident.span().start().line,
        });
    }
}


fn scoped_key(scope: &str, name: &str) -> String {
    if scope.is_empty() {
        name.to_string()
    } else {
        format!("{scope}::{name}")
    }
}


#[derive(Default)]
pub(crate) struct TypeImports {
    bindings: HashMap<String, HashMap<String, Vec<String>>>,
}

impl TypeImports {
    fn add(&mut self, scope: &str, name: String, path: Vec<String>) {
        self.bindings
            .entry(scope.to_string())
            .or_default()
            .insert(name, path);
    }

    fn lookup(&self, scope: &str, name: &str) -> Option<(String, Vec<String>)> {
        let mut current = scope.to_string();
        loop {
            if let Some(bindings) = self.bindings.get(&current) {
                if let Some(path) = bindings.get(name) {
                    return Some((current, path.clone()));
                }
            }
            if current.is_empty() {
                return None;
            }
            current = current
                .rsplit_once("::")
                .map_or_else(String::new, |(parent, _)| parent.to_string());
        }
    }
}


fn collect_imports(items: &[Item], scope: &str, imports: &mut TypeImports) {
    for item in items {
        let Item::Use(use_item) = item else {
            continue;
        };
        let mut prefix = Vec::new();
        collect_use_tree(&use_item.tree, &mut prefix, scope, imports);
    }
}


fn collect_use_tree(
    tree: &UseTree,
    prefix: &mut Vec<String>,
    scope: &str,
    imports: &mut TypeImports,
) {
    match tree {
        UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            collect_use_tree(&path.tree, prefix, scope, imports);
            prefix.pop();
        }
        UseTree::Name(name) => {
            let imported_name = name.ident.to_string();
            if imported_name == "self" {
                let Some(alias) = prefix.last().cloned() else {
                    return;
                };
                imports.add(scope, alias, prefix.clone());
            } else {
                prefix.push(imported_name.clone());
                imports.add(scope, imported_name, prefix.clone());
                prefix.pop();
            }
        }
        UseTree::Rename(rename) => {
            prefix.push(rename.ident.to_string());
            imports.add(scope, rename.rename.to_string(), prefix.clone());
            prefix.pop();
        }
        UseTree::Glob(_) => {}
        UseTree::Group(group) => {
            for item in &group.items {
                collect_use_tree(item, prefix, scope, imports);
            }
        }
    }
}


fn scope_parent(scope: &str) -> String {
    scope
        .rsplit_once("::")
        .map_or_else(String::new, |(parent, _)| parent.to_string())
}


fn resolve_type_path(path: &str, scope: &str, imports: &TypeImports) -> String {
    let segments: Vec<_> = path.split("::").map(str::to_string).collect();
    let mut seen = HashSet::new();
    resolve_type_segments(&segments, scope, imports, &mut seen).join("::")
}


fn resolve_type_segments(
    segments: &[String],
    scope: &str,
    imports: &TypeImports,
    seen: &mut HashSet<String>,
) -> Vec<String> {
    let Some(first) = segments.first() else {
        return Vec::new();
    };
    match first.as_str() {
        "crate" => resolve_type_segments(&segments[1..], "", imports, seen),
        "self" => resolve_type_segments(&segments[1..], scope, imports, seen),
        "super" => resolve_type_segments(&segments[1..], &scope_parent(scope), imports, seen),
        _ => {
            if let Some((binding_scope, binding_path)) = imports.lookup(scope, first) {
                let marker = format!("{binding_scope}::{first}");
                if seen.insert(marker) {
                    let mut resolved =
                        resolve_type_segments(&binding_path, &binding_scope, imports, seen);
                    resolved.extend(segments.iter().skip(1).cloned());
                    return resolved;
                }
            }
            let mut resolved: Vec<_> = scope
                .split("::")
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect();
            resolved.extend(segments.iter().cloned());
            resolved
        }
    }
}


pub(crate) fn collect_items<'a>(
    items: &'a [Item],
    scope: &str,
    types: &mut HashMap<String, TypeModel<'a>>,
    functions: &mut Vec<FnModel<'a>>,
    imports: &mut TypeImports,
    ignore_tests: bool,
) {
    collect_imports(items, scope, imports);
    for item in items {
        match item {
            Item::Struct(s) => insert_struct(types, s, scope),
            Item::Enum(e) => insert_enum(types, e, scope),
            Item::Union(u) => insert_union(types, u, scope),
            Item::Trait(t) => insert_trait(types, t, functions, scope),
            Item::Fn(f) => functions.push(fn_from_item(f)),
            Item::Impl(im) => attach_impl(types, functions, im, scope, imports, ignore_tests),
            Item::Mod(module) => {
                collect_module_items(module, scope, types, functions, imports, ignore_tests)
            }
            _ => {}
        }
    }
}


pub(crate) fn collect_module_items<'a>(
    module: &'a syn::ItemMod,
    scope: &str,
    types: &mut HashMap<String, TypeModel<'a>>,
    functions: &mut Vec<FnModel<'a>>,
    imports: &mut TypeImports,
    ignore_tests: bool,
) {
    if let Some((_, nested)) = &module.content {
        let mod_name = module.ident.to_string();
        let new_scope = scoped_key(scope, &mod_name);
        collect_items(nested, &new_scope, types, functions, imports, ignore_tests);
    }
}


pub(crate) struct TypeDefinition {
    pub(crate) key: String,
    pub(crate) name: String,
    pub(crate) node_type: &'static str,
    pub(crate) begin_line: usize,
    pub(crate) end_line: usize,
    pub(crate) field_count: usize,
    pub(crate) public_fields: usize,
    pub(crate) fields: Vec<FieldInfo>,
}


pub(crate) fn upsert_type<'a>(types: &mut HashMap<String, TypeModel<'a>>, definition: TypeDefinition) {
    let TypeDefinition {
        key,
        name,
        node_type,
        begin_line,
        end_line,
        field_count,
        public_fields,
        fields,
    } = definition;
    match types.entry(key.clone()) {
        std::collections::hash_map::Entry::Occupied(mut entry) => {
            let existing = entry.get_mut();
            existing.node_type = node_type.to_string();
            existing.begin_line = begin_line;
            existing.end_line = end_line;
            existing.field_count = field_count;
            existing.public_fields = public_fields;
            existing.has_declaration = true;
            if !fields.is_empty() {
                existing.fields = fields;
            }
        }
        std::collections::hash_map::Entry::Vacant(entry) => {
            entry.insert(TypeModel {
                key,
                name,
                node_type: node_type.to_string(),
                begin_line,
                end_line,
                field_count,
                public_fields,
                fields,
                methods: Vec::new(),
                has_declaration: true,
            });
        }
    }
}


pub(crate) fn field_infos(fields: &Fields) -> Vec<FieldInfo> {
    match fields {
        Fields::Named(n) => n
            .named
            .iter()
            .filter_map(|f| {
                let ident = f.ident.as_ref()?;
                Some(FieldInfo {
                    name: ident.to_string(),
                    begin_line: ident.span().start().line,
                    type_names: type_names_in(&f.ty),
                })
            })
            .collect(),
        Fields::Unnamed(u) => u
            .unnamed
            .iter()
            .enumerate()
            .map(|(i, f)| FieldInfo {
                name: i.to_string(),
                begin_line: f.ty.span().start().line,
                type_names: type_names_in(&f.ty),
            })
            .collect(),
        Fields::Unit => Vec::new(),
    }
}


pub(crate) fn insert_struct<'a>(
    types: &mut HashMap<String, TypeModel<'a>>,
    s: &'a ItemStruct,
    scope: &str,
) {
    let (field_count, public_fields) = field_stats(&s.fields);
    let name = s.ident.to_string();
    let key = scoped_key(scope, &name);
    upsert_type(
        types,
        TypeDefinition {
            key,
            name,
            node_type: "struct",
            begin_line: s.struct_token.span().start().line,
            end_line: s.span().end().line,
            field_count,
            public_fields,
            fields: field_infos(&s.fields),
        },
    );
}


pub(crate) fn insert_enum<'a>(
    types: &mut HashMap<String, TypeModel<'a>>,
    e: &'a ItemEnum,
    scope: &str,
) {
    let mut fields = Vec::new();
    for v in &e.variants {
        for f in field_infos(&v.fields) {
            fields.push(f);
        }
    }
    let name = e.ident.to_string();
    let key = scoped_key(scope, &name);
    upsert_type(
        types,
        TypeDefinition {
            key,
            name,
            node_type: "enum",
            begin_line: e.enum_token.span().start().line,
            end_line: e.span().end().line,
            field_count: e.variants.len(),
            public_fields: 0,
            fields,
        },
    );
}


pub(crate) fn insert_union<'a>(
    types: &mut HashMap<String, TypeModel<'a>>,
    u: &'a ItemUnion,
    scope: &str,
) {
    let field_count = u.fields.named.len();
    let public_fields = u.fields.named.iter().filter(|f| is_public(&f.vis)).count();
    let fields = u
        .fields
        .named
        .iter()
        .filter_map(|f| {
            let ident = f.ident.as_ref()?;
            Some(FieldInfo {
                name: ident.to_string(),
                begin_line: ident.span().start().line,
                type_names: type_names_in(&f.ty),
            })
        })
        .collect();
    let name = u.ident.to_string();
    let key = scoped_key(scope, &name);
    upsert_type(
        types,
        TypeDefinition {
            key,
            name,
            node_type: "union",
            begin_line: u.union_token.span().start().line,
            end_line: u.span().end().line,
            field_count,
            public_fields,
            fields,
        },
    );
}


pub(crate) fn insert_trait<'a>(
    types: &mut HashMap<String, TypeModel<'a>>,
    t: &'a ItemTrait,
    functions: &mut Vec<FnModel<'a>>,
    scope: &str,
) {
    let trait_public = is_public(&t.vis);
    let mut methods = Vec::new();
    let name = t.ident.to_string();
    let key = scoped_key(scope, &name);
    for item in &t.items {
        if let syn::TraitItem::Fn(m) = item {
            let begin = m.sig.fn_token.span().start().line;
            let end = m.span().end().line;
            let body = m.default.as_ref();
            let method_name = m.sig.ident.to_string();
            methods.push(MethodRef {
                name: method_name.clone(),
                begin_line: begin,
                end_line: end,
                is_public: trait_public,
                body,
            });
            functions.push(FnModel {
                name: method_name,
                parent: Some(name.clone()),
                parent_key: Some(key.clone()),
                begin_line: begin,
                end_line: end,
                param_count: count_params(&m.sig.inputs),
                bool_params: bool_params(&m.sig.inputs),
                body,
                returns_bool: returns_bool(&m.sig.output),
                dep_types: sig_dep_types(&m.sig),
                counts_for_type_metrics: false,
                signature: &m.sig,
                in_trait_impl: false,
            });
        }
    }
    types
        .entry(key.clone())
        .and_modify(|existing| {
            existing.node_type = "trait".to_string();
            existing.begin_line = t.trait_token.span().start().line;
            existing.end_line = t.span().end().line;
            existing.public_fields = 0;
            existing.has_declaration = true;
            existing.methods.append(&mut methods);
        })
        .or_insert_with(|| TypeModel {
            key,
            name,
            node_type: "trait".to_string(),
            begin_line: t.trait_token.span().start().line,
            end_line: t.span().end().line,
            field_count: 0,
            public_fields: 0,
            fields: Vec::new(),
            methods,
            has_declaration: true,
        });
}


pub(crate) fn fn_from_item(f: &ItemFn) -> FnModel<'_> {
    FnModel {
        name: f.sig.ident.to_string(),
        parent: None,
        parent_key: None,
        begin_line: f.sig.fn_token.span().start().line,
        end_line: f.span().end().line,
        param_count: count_params(&f.sig.inputs),
        bool_params: bool_params(&f.sig.inputs),
        body: Some(&f.block),
        returns_bool: returns_bool(&f.sig.output),
        dep_types: sig_dep_types(&f.sig),
        counts_for_type_metrics: false,
        signature: &f.sig,
        in_trait_impl: false,
    }
}


/// Key of the type that an `impl` block belongs to. A key that the model
/// already holds wins, so an `impl` block joins the declaration of its type.
fn impl_type_key(
    types: &HashMap<String, TypeModel<'_>>,
    im: &ItemImpl,
    scope: &str,
    imports: &TypeImports,
) -> String {
    let full_path = full_type_path_from_type(&im.self_ty);
    let resolved_path = resolve_type_path(&full_path, scope, imports);
    if types.contains_key(&resolved_path) {
        return resolved_path;
    }
    let scoped = scoped_key(scope, &full_path);
    if types.contains_key(&scoped) {
        return scoped;
    }
    if types.contains_key(&full_path) {
        return full_path;
    }
    resolved_path
}


fn attach_impl_method<'a>(
    types: &mut HashMap<String, TypeModel<'a>>,
    functions: &mut Vec<FnModel<'a>>,
    method: &'a syn::ImplItemFn,
    owner: (&str, &str),
    inherent: bool,
) {
    let (ty_name, key) = owner;
    let begin_line = method.sig.fn_token.span().start().line;
    let end_line = method.span().end().line;
    let name = method.sig.ident.to_string();
    if inherent {
        if let Some(type_model) = types.get_mut(key) {
            type_model.methods.push(MethodRef {
                name: name.clone(),
                begin_line,
                end_line,
                is_public: is_public(&method.vis),
                body: Some(&method.block),
            });
        }
    }
    functions.push(FnModel {
        name,
        parent: Some(ty_name.to_string()),
        parent_key: Some(key.to_string()),
        begin_line,
        end_line,
        param_count: count_params(&method.sig.inputs),
        bool_params: bool_params(&method.sig.inputs),
        body: Some(&method.block),
        returns_bool: returns_bool(&method.sig.output),
        dep_types: sig_dep_types(&method.sig),
        counts_for_type_metrics: inherent,
        signature: &method.sig,
        in_trait_impl: !inherent,
    });
}


pub(crate) fn attach_impl<'a>(
    types: &mut HashMap<String, TypeModel<'a>>,
    functions: &mut Vec<FnModel<'a>>,
    im: &'a ItemImpl,
    scope: &str,
    imports: &TypeImports,
    ignore_tests: bool,
) {
    let ty_name = type_name_from_path(&im.self_ty);
    // A test-only `impl` block holds no production code. Keep the whole block
    // out of the model, so no type metric counts its methods.
    if ty_name.is_empty() || (ignore_tests && is_test_module(&im.attrs)) {
        return;
    }
    let key = impl_type_key(types, im, scope, imports);
    types.entry(key.clone()).or_insert_with(|| TypeModel {
        key: key.clone(),
        name: ty_name.clone(),
        node_type: "struct".to_string(),
        begin_line: im.impl_token.span().start().line,
        end_line: im.span().end().line,
        field_count: 0,
        public_fields: 0,
        fields: Vec::new(),
        methods: Vec::new(),
        has_declaration: false,
    });
    let inherent = im.trait_.is_none();
    for item in &im.items {
        let syn::ImplItem::Fn(method) = item else {
            continue;
        };
        // A method with its own test-only `cfg` attribute drops the same way.
        if ignore_tests && is_test_module(&method.attrs) {
            continue;
        }
        attach_impl_method(types, functions, method, (&ty_name, &key), inherent);
    }
}


pub(crate) fn sig_dep_types(sig: &syn::Signature) -> Vec<String> {
    let mut names = Vec::new();
    for input in &sig.inputs {
        if let FnArg::Typed(pt) = input {
            names.extend(type_names_in(&pt.ty));
        }
    }
    if let ReturnType::Type(_, ty) = &sig.output {
        names.extend(type_names_in(ty));
    }
    names
}


pub(crate) fn type_names_in(ty: &syn::Type) -> Vec<String> {
    let mut out = Vec::new();
    collect_type_names(ty, &mut out);
    out
}


pub(crate) fn collect_type_names(ty: &syn::Type, out: &mut Vec<String>) {
    let mut collector = TypeNameCollector { names: out };
    collector.visit_type(ty);
}


pub(crate) struct TypeNameCollector<'a> {
    pub(crate) names: &'a mut Vec<String>,
}


impl<'ast> Visit<'ast> for TypeNameCollector<'_> {
    fn visit_type_path(&mut self, node: &'ast syn::TypePath) {
        if let Some(segment) = node.path.segments.last() {
            self.names.push(segment.ident.to_string());
        }
    }
}


pub(crate) fn is_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "bool"
            | "char"
            | "str"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f32"
            | "f64"
            | "Self"
            | "self"
    )
}


#[derive(Default)]
pub(crate) struct StaticMutCollector {
    pub(crate) static_muts: Vec<StaticMutSite>,
    pub(crate) mutated: HashSet<String>,
    scope: Vec<String>,
}


impl<'ast> Visit<'ast> for StaticMutCollector {
    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        if !matches!(node.mutability, syn::StaticMutability::None) {
            let name = node.ident.to_string();
            self.static_muts.push(StaticMutSite {
                key: qualified_name(&self.scope, &name),
                name,
                begin_line: node.ident.span().start().line,
            });
        }
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let Some((_, items)) = &node.content else {
            return;
        };
        self.scope.push(node.ident.to_string());
        for item in items {
            self.visit_item(item);
        }
        self.scope.pop();
    }

    fn visit_expr_assign(&mut self, node: &'ast syn::ExprAssign) {
        collect_mutated_static_place(&node.left, &self.scope, &mut self.mutated);
        syn::visit::visit_expr_assign(self, node);
    }

    fn visit_expr_binary(&mut self, node: &'ast syn::ExprBinary) {
        if matches!(
            node.op,
            syn::BinOp::AddAssign(_)
                | syn::BinOp::SubAssign(_)
                | syn::BinOp::MulAssign(_)
                | syn::BinOp::DivAssign(_)
                | syn::BinOp::RemAssign(_)
                | syn::BinOp::BitXorAssign(_)
                | syn::BinOp::BitAndAssign(_)
                | syn::BinOp::BitOrAssign(_)
                | syn::BinOp::ShlAssign(_)
                | syn::BinOp::ShrAssign(_)
        ) {
            collect_mutated_static_place(&node.left, &self.scope, &mut self.mutated);
        }
        syn::visit::visit_expr_binary(self, node);
    }
}


fn qualified_name(scope: &[String], name: &str) -> String {
    if scope.is_empty() {
        name.to_string()
    } else {
        format!("{}::{name}", scope.join("::"))
    }
}


fn static_path_key(path: &syn::Path, scope: &[String]) -> Option<String> {
    let mut components = if path.leading_colon.is_some() {
        Vec::new()
    } else {
        scope.to_vec()
    };
    let mut has_name = false;
    for segment in &path.segments {
        let name = segment.ident.to_string();
        match name.as_str() {
            "crate" => components.clear(),
            "self" => {}
            "super" => {
                components.pop();
            }
            _ => {
                components.push(name);
                has_name = true;
            }
        }
    }
    has_name.then(|| components.join("::"))
}


fn collect_mutated_static_place(
    expr: &syn::Expr,
    scope: &[String],
    mutated: &mut HashSet<String>,
) {
    match expr {
        syn::Expr::Path(p) => {
            if p.qself.is_none() {
                if let Some(key) = static_path_key(&p.path, scope) {
                    mutated.insert(key);
                }
            }
        }
        syn::Expr::Field(f) => collect_mutated_static_place(&f.base, scope, mutated),
        syn::Expr::Index(i) => collect_mutated_static_place(&i.expr, scope, mutated),
        syn::Expr::Paren(p) => collect_mutated_static_place(&p.expr, scope, mutated),
        syn::Expr::Unary(u) if matches!(u.op, syn::UnOp::Deref(_)) => {
            collect_mutated_static_place(&u.expr, scope, mutated)
        }
        _ => {}
    }
}


/// Collects the names of statics that can hold a different value between
/// calls: `static mut` items, statics with an interior-mutability type, and
/// `thread_local!` keys. An immutable static of a plain type acts as a
/// constant, so it does not enter the set.
#[derive(Default)]
pub(crate) struct SharedStaticCollector {
    pub(crate) names: HashSet<String>,
}


impl<'ast> Visit<'ast> for SharedStaticCollector {
    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        let mutable = !matches!(node.mutability, syn::StaticMutability::None);
        if mutable || has_interior_mutable_type(&node.ty) {
            self.names.insert(node.ident.to_string());
        }
        syn::visit::visit_item_static(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if node.path.segments.last().is_some_and(|segment| segment.ident == "thread_local") {
            collect_thread_local_keys(node.tokens.clone(), &mut self.names);
        }
    }
}


/// True when `ty` or one of its generic arguments is interior-mutable, for
/// example `LazyLock<Mutex<T>>`.
fn has_interior_mutable_type(ty: &syn::Type) -> bool {
    let mut finder = InteriorMutableFinder { found: false };
    finder.visit_type(ty);
    finder.found
}


struct InteriorMutableFinder {
    found: bool,
}


impl<'ast> Visit<'ast> for InteriorMutableFinder {
    fn visit_type_path(&mut self, node: &'ast syn::TypePath) {
        if let Some(segment) = node.path.segments.last() {
            self.found |= is_interior_mutable(&segment.ident.to_string());
        }
        syn::visit::visit_type_path(self, node);
    }
}


fn is_interior_mutable(type_name: &str) -> bool {
    type_name.starts_with("Atomic")
        || matches!(
            type_name,
            "Cell" | "RefCell" | "UnsafeCell" | "Mutex" | "RwLock" | "OnceCell" | "OnceLock"
        )
}


fn collect_thread_local_keys(tokens: proc_macro2::TokenStream, names: &mut HashSet<String>) {
    let mut after_static = false;
    for token in tokens {
        if let proc_macro2::TokenTree::Ident(ident) = token {
            if after_static {
                names.insert(ident.to_string());
            }
            after_static = ident == "static";
        } else {
            after_static = false;
        }
    }
}
