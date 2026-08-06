//! File model: types, functions, use-def, and related collectors.

mod build;
mod use_def;

use std::collections::{HashMap, HashSet};

use syn::visit::Visit;
use syn::{Fields, FnArg, Pat, PatType, ReturnType};

use super::helpers::is_public;

use self::build::{
    collect_items, BindingCollector, DuplicateKeyCollector, StaticMutCollector,
};
use self::use_def::UseDefCollector;

pub(crate) use self::use_def::path_single_ident;

pub(crate) fn count_params(inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>) -> usize {
    inputs
        .iter()
        .filter(|arg| match arg {
            FnArg::Receiver(_) => false,
            FnArg::Typed(PatType { pat, .. }) => !matches!(**pat, Pat::Wild(_)),
        })
        .count()
}


pub(crate) fn bool_params(inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>) -> Vec<BoolParam> {
    let mut out = Vec::new();
    for arg in inputs {
        let FnArg::Typed(PatType { pat, ty, .. }) = arg else {
            continue;
        };
        if type_name_from_path(ty) != "bool" {
            continue;
        }
        collect_bool_param_names(pat, &mut out);
    }
    out
}


pub(crate) fn collect_bool_param_names(pat: &Pat, out: &mut Vec<BoolParam>) {
    // bool_params only invokes this when the parameter type path is `bool`, so
    // tuple/paren destructuring patterns cannot appear here.
    match pat {
        Pat::Ident(id) => out.push(BoolParam {
            name: id.ident.to_string(),
            begin_line: id.ident.span().start().line,
        }),
        Pat::Reference(r) => collect_bool_param_names(&r.pat, out),
        _ => {}
    }
}


pub(crate) fn field_stats(fields: &Fields) -> (usize, usize) {
    match fields {
        Fields::Named(n) => {
            let total = n.named.len();
            let public = n.named.iter().filter(|f| is_public(&f.vis)).count();
            (total, public)
        }
        Fields::Unnamed(u) => {
            let total = u.unnamed.len();
            let public = u.unnamed.iter().filter(|f| is_public(&f.vis)).count();
            (total, public)
        }
        Fields::Unit => (0, 0),
    }
}


pub(crate) fn type_name_from_path(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(p) => p
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}


pub(crate) fn returns_bool(output: &ReturnType) -> bool {
    match output {
        ReturnType::Type(_, ty) => type_name_from_path(ty) == "bool",
        ReturnType::Default => false,
    }
}


pub(crate) struct NamedBinding {
    pub(crate) name: String,
    pub(crate) begin_line: usize,
}


pub(crate) struct BoolParam {
    pub(crate) name: String,
    pub(crate) begin_line: usize,
}


pub(crate) struct DuplicateKey {
    pub(crate) display: String,
    pub(crate) line: usize,
    pub(crate) first_line: usize,
}


pub(crate) struct FnModel<'a> {
    pub(crate) name: String,
    pub(crate) parent: Option<String>,
    pub(crate) begin_line: usize,
    pub(crate) end_line: usize,
    pub(crate) param_count: usize,
    pub(crate) bool_params: Vec<BoolParam>,
    pub(crate) body: Option<&'a syn::Block>,
    pub(crate) returns_bool: bool,
    pub(crate) dep_types: Vec<String>,
    pub(crate) counts_for_type_metrics: bool,
}


impl FnModel<'_> {
    pub(crate) fn kind_label(&self) -> &'static str {
        if self.parent.is_some() {
            "method"
        } else {
            "function"
        }
    }
}


pub(crate) struct MethodRef<'a> {
    pub(crate) name: String,
    pub(crate) begin_line: usize,
    pub(crate) end_line: usize,
    pub(crate) is_public: bool,
    pub(crate) body: Option<&'a syn::Block>,
}

#[derive(Clone)]


pub(crate) struct FieldInfo {
    pub(crate) name: String,
    pub(crate) begin_line: usize,
    pub(crate) type_names: Vec<String>,
}


pub(crate) struct TypeModel<'a> {
    pub(crate) name: String,
    pub(crate) node_type: String,
    pub(crate) begin_line: usize,
    pub(crate) end_line: usize,
    pub(crate) field_count: usize,
    pub(crate) public_fields: usize,
    pub(crate) fields: Vec<FieldInfo>,
    pub(crate) methods: Vec<MethodRef<'a>>,
}


pub(crate) struct FileModel<'a> {
    pub(crate) src: &'a str,
    pub(crate) functions: Vec<FnModel<'a>>,
    pub(crate) types: Vec<TypeModel<'a>>,
    pub(crate) variables: Vec<NamedBinding>,
    pub(crate) constants: Vec<NamedBinding>,
    pub(crate) usage: UseDefModel,
    pub(crate) duplicate_struct_keys: Vec<DuplicateKey>,
    pub(crate) static_muts: Vec<NamedSite>,
    pub(crate) mutated_statics: HashSet<String>,
}

#[derive(Default)]


pub(crate) struct UseDefModel {
    pub(crate) locals: Vec<NamedSite>,
    pub(crate) params: Vec<NamedSite>,
    pub(crate) private_fields: Vec<NamedSite>,
    pub(crate) private_methods: Vec<NamedSite>,
    pub(crate) ident_reads: HashSet<String>,
    pub(crate) field_reads: HashSet<String>,
    pub(crate) method_calls: HashSet<String>,
}

#[derive(Clone, Debug)]


pub(crate) struct NamedSite {
    pub(crate) name: String,
    pub(crate) begin_line: usize,
}


impl<'a> FileModel<'a> {
    pub(crate) fn from_file(file: &'a syn::File, src: &'a str) -> Self {
        let mut types: HashMap<String, TypeModel<'a>> = HashMap::new();
        let mut functions = Vec::new();
        collect_items(&file.items, &mut types, &mut functions);

        let mut binder = BindingCollector {
            variables: Vec::new(),
            constants: Vec::new(),
        };
        binder.visit_file(file);

        let mut usage = UseDefCollector::new();
        usage.visit_file(file);

        let mut dup = DuplicateKeyCollector::default();
        dup.visit_file(file);

        let mut statics = StaticMutCollector::default();
        statics.visit_file(file);

        let types: Vec<_> = types.into_values().collect();
        Self {
            src,
            functions,
            types,
            variables: binder.variables,
            constants: binder.constants,
            usage: usage.into_model(),
            duplicate_struct_keys: dup.keys,
            static_muts: statics.static_muts,
            mutated_statics: statics.mutated,
        }
    }
}



pub(crate) use self::build::is_builtin_type;
