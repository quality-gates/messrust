//! AST walkers that build the file model.

use std::collections::{HashMap, HashSet};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Fields, FnArg, Item, ItemEnum, ItemFn, ItemImpl, ItemStruct, ItemTrait, ItemUnion, Member,
    ReturnType,
};

use crate::analyze::helpers::is_public;

use super::use_def::{is_binding_name, path_single_ident};
use super::{
    bool_params, count_params, field_stats, returns_bool, type_name_from_path, DuplicateKey,
    FieldInfo, FnModel, MethodRef, NamedBinding, NamedSite, TypeModel,
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


pub(crate) fn collect_items<'a>(
    items: &'a [Item],
    types: &mut HashMap<String, TypeModel<'a>>,
    functions: &mut Vec<FnModel<'a>>,
) {
    for item in items {
        match item {
            Item::Struct(s) => insert_struct(types, s),
            Item::Enum(e) => insert_enum(types, e),
            Item::Union(u) => insert_union(types, u),
            Item::Trait(t) => insert_trait(types, t, functions),
            Item::Fn(f) => functions.push(fn_from_item(f)),
            Item::Impl(im) => attach_impl(types, functions, im),
            Item::Mod(module) => collect_module_items(module, types, functions),
            _ => {}
        }
    }
}


pub(crate) fn collect_module_items<'a>(
    module: &'a syn::ItemMod,
    types: &mut HashMap<String, TypeModel<'a>>,
    functions: &mut Vec<FnModel<'a>>,
) {
    if let Some((_, nested)) = &module.content {
        collect_items(nested, types, functions);
    }
}


pub(crate) struct TypeDefinition {
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
        name,
        node_type,
        begin_line,
        end_line,
        field_count,
        public_fields,
        fields,
    } = definition;
    match types.entry(name.clone()) {
        std::collections::hash_map::Entry::Occupied(mut entry) => {
            let existing = entry.get_mut();
            existing.node_type = node_type.to_string();
            existing.begin_line = begin_line;
            existing.end_line = end_line;
            existing.field_count = field_count;
            existing.public_fields = public_fields;
            if !fields.is_empty() {
                existing.fields = fields;
            }
        }
        std::collections::hash_map::Entry::Vacant(entry) => {
            entry.insert(TypeModel {
                name,
                node_type: node_type.to_string(),
                begin_line,
                end_line,
                field_count,
                public_fields,
                fields,
                methods: Vec::new(),
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


pub(crate) fn insert_struct<'a>(types: &mut HashMap<String, TypeModel<'a>>, s: &'a ItemStruct) {
    let (field_count, public_fields) = field_stats(&s.fields);
    upsert_type(
        types,
        TypeDefinition {
            name: s.ident.to_string(),
            node_type: "struct",
            begin_line: s.struct_token.span().start().line,
            end_line: s.span().end().line,
            field_count,
            public_fields,
            fields: field_infos(&s.fields),
        },
    );
}


pub(crate) fn insert_enum<'a>(types: &mut HashMap<String, TypeModel<'a>>, e: &'a ItemEnum) {
    let mut fields = Vec::new();
    for v in &e.variants {
        for f in field_infos(&v.fields) {
            fields.push(f);
        }
    }
    upsert_type(
        types,
        TypeDefinition {
            name: e.ident.to_string(),
            node_type: "enum",
            begin_line: e.enum_token.span().start().line,
            end_line: e.span().end().line,
            field_count: e.variants.len(),
            public_fields: 0,
            fields,
        },
    );
}


pub(crate) fn insert_union<'a>(types: &mut HashMap<String, TypeModel<'a>>, u: &'a ItemUnion) {
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
    upsert_type(
        types,
        TypeDefinition {
            name: u.ident.to_string(),
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
) {
    let trait_public = is_public(&t.vis);
    let mut methods = Vec::new();
    for item in &t.items {
        if let syn::TraitItem::Fn(m) = item {
            let begin = m.sig.fn_token.span().start().line;
            let end = m.span().end().line;
            let body = m.default.as_ref();
            let name = m.sig.ident.to_string();
            methods.push(MethodRef {
                name: name.clone(),
                begin_line: begin,
                end_line: end,
                is_public: trait_public,
                body,
            });
            functions.push(FnModel {
                name,
                parent: Some(t.ident.to_string()),
                begin_line: begin,
                end_line: end,
                param_count: count_params(&m.sig.inputs),
                bool_params: bool_params(&m.sig.inputs),
                body,
                returns_bool: returns_bool(&m.sig.output),
                dep_types: sig_dep_types(&m.sig),
                counts_for_type_metrics: false,
            });
        }
    }
    let name = t.ident.to_string();
    types
        .entry(name.clone())
        .and_modify(|existing| {
            existing.node_type = "trait".to_string();
            existing.begin_line = t.trait_token.span().start().line;
            existing.end_line = t.span().end().line;
            existing.public_fields = 0;
            existing.methods.append(&mut methods);
        })
        .or_insert_with(|| TypeModel {
            name,
            node_type: "trait".to_string(),
            begin_line: t.trait_token.span().start().line,
            end_line: t.span().end().line,
            field_count: 0,
            public_fields: 0,
            fields: Vec::new(),
            methods,
        });
}


pub(crate) fn fn_from_item(f: &ItemFn) -> FnModel<'_> {
    FnModel {
        name: f.sig.ident.to_string(),
        parent: None,
        begin_line: f.sig.fn_token.span().start().line,
        end_line: f.span().end().line,
        param_count: count_params(&f.sig.inputs),
        bool_params: bool_params(&f.sig.inputs),
        body: Some(&f.block),
        returns_bool: returns_bool(&f.sig.output),
        dep_types: sig_dep_types(&f.sig),
        counts_for_type_metrics: false,
    }
}


pub(crate) fn attach_impl<'a>(
    types: &mut HashMap<String, TypeModel<'a>>,
    functions: &mut Vec<FnModel<'a>>,
    im: &'a ItemImpl,
) {
    let ty_name = type_name_from_path(&im.self_ty);
    if ty_name.is_empty() {
        return;
    }
    types.entry(ty_name.clone()).or_insert_with(|| TypeModel {
        name: ty_name.clone(),
        node_type: "struct".to_string(),
        begin_line: im.impl_token.span().start().line,
        end_line: im.span().end().line,
        field_count: 0,
        public_fields: 0,
        fields: Vec::new(),
        methods: Vec::new(),
    });
    for item in &im.items {
        if let syn::ImplItem::Fn(m) = item {
            let begin = m.sig.fn_token.span().start().line;
            let end = m.span().end().line;
            let name = m.sig.ident.to_string();
            let is_pub = is_public(&m.vis);
            if im.trait_.is_none() {
                types.get_mut(&ty_name).unwrap().methods.push(MethodRef {
                    name: name.clone(),
                    begin_line: begin,
                    end_line: end,
                    is_public: is_pub,
                    body: Some(&m.block),
                });
            }
            functions.push(FnModel {
                name,
                parent: Some(ty_name.clone()),
                begin_line: begin,
                end_line: end,
                param_count: count_params(&m.sig.inputs),
                bool_params: bool_params(&m.sig.inputs),
                body: Some(&m.block),
                returns_bool: returns_bool(&m.sig.output),
                dep_types: sig_dep_types(&m.sig),
                counts_for_type_metrics: im.trait_.is_none(),
            });
        }
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
    pub(crate) static_muts: Vec<NamedSite>,
    pub(crate) mutated: HashSet<String>,
    pub(crate) assign_lhs: bool,
}


impl<'ast> Visit<'ast> for StaticMutCollector {
    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        if !matches!(node.mutability, syn::StaticMutability::None) {
            self.static_muts.push(NamedSite {
                name: node.ident.to_string(),
                begin_line: node.ident.span().start().line,
            });
        }
    }

    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        if self.assign_lhs {
            if let Some(ident) = path_single_ident(node) {
                self.mutated.insert(ident);
            }
        }
    }

    fn visit_expr_assign(&mut self, node: &'ast syn::ExprAssign) {
        self.assign_lhs = true;
        self.visit_expr(&node.left);
        self.assign_lhs = false;
        self.visit_expr(&node.right);
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
            self.assign_lhs = true;
            self.visit_expr(&node.left);
            self.assign_lhs = false;
            self.visit_expr(&node.right);
        }
    }
}

