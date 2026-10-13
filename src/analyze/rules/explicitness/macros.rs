use std::collections::BTreeMap;

use syn::parse::Parser;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::Expr;

use crate::analyze::model::FileModel;

#[derive(Default)]
pub(super) struct Arguments {
    items: BTreeMap<(usize, usize), Vec<Expr>>,
}

impl Arguments {
    pub(super) fn collect(model: &FileModel<'_>) -> Self {
        let mut args = Self::default();
        for function in &model.functions {
            if let Some(body) = function.body {
                MacroCollector { args: &mut args }.visit_block(body);
            }
        }
        args
    }

    pub(super) fn get(&self, node: &syn::Macro) -> &[Expr] {
        self.items.get(&key(node)).map_or(&[], Vec::as_slice)
    }
}

struct MacroCollector<'a> {
    args: &'a mut Arguments,
}

impl<'ast> Visit<'ast> for MacroCollector<'_> {
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if self.args.items.contains_key(&key(node)) {
            return;
        }
        let args = parse(node);
        self.args.items.insert(key(node), Vec::new());
        for expr in &args {
            MacroCollector { args: self.args }.visit_expr(expr);
        }
        self.args.items.insert(key(node), args);
    }
}

fn key(node: &syn::Macro) -> (usize, usize) {
    let start = node.span().start();
    (start.line, start.column)
}

fn parse(node: &syn::Macro) -> Vec<Expr> {
    let parser = syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated;
    if let Ok(args) = parser.parse2(node.tokens.clone()) {
        return args.into_iter().collect();
    }
    if node.path.is_ident("vec") {
        let parser = |input: syn::parse::ParseStream<'_>| {
            let first: Expr = input.parse()?;
            input.parse::<syn::Token![;]>()?;
            let count: Expr = input.parse()?;
            Ok(vec![first, count])
        };
        if let Ok(args) = parser.parse2(node.tokens.clone()) {
            return args;
        }
    }
    Vec::new()
}
