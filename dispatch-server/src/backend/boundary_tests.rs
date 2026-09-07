//! Parse production Rust so test fixtures and literals cannot hide or trigger boundary violations.
use assertr::prelude::*;
use std::{collections::BTreeSet, fs, path::Path};
use syn::visit::{self, Visit};

fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if attr.path().is_ident("test")
            || attr
                .path()
                .segments
                .last()
                .is_some_and(|part| part.ident == "test")
        {
            return true;
        }
        attr.path().is_ident("cfg")
            && attr.parse_args::<syn::Meta>().is_ok_and(|meta| match meta {
                syn::Meta::Path(path) => path.is_ident("test"),
                syn::Meta::List(list) if list.path.is_ident("all") => list
                    .parse_args_with(
                        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                    )
                    .is_ok_and(|items| {
                        items.iter().any(
                            |meta| matches!(meta,syn::Meta::Path(path) if path.is_ident("test")),
                        )
                    }),
                _ => false,
            })
    })
}
#[derive(Default)]
struct Symbols(BTreeSet<String>);
impl<'ast> Visit<'ast> for Symbols {
    fn visit_ident(&mut self, ident: &'ast syn::Ident) {
        self.0.insert(ident.to_string());
    }
}
struct Boundaries<'a> {
    path: &'a Path,
    persistence: bool,
    composition: bool,
    transport: bool,
    hooks: bool,
    violations: Vec<String>,
    controller_handlers: usize,
}
impl Boundaries<'_> {
    fn reject(&mut self, message: impl AsRef<str>) {
        self.violations
            .push(format!("{}: {}", self.path.display(), message.as_ref()));
    }
}
impl<'ast> Visit<'ast> for Boundaries<'_> {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if !test_only(&item.attrs) && item.content.is_some() {
            visit::visit_item_mod(self, item);
        }
    }
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if test_only(&item.attrs) {
            return;
        }
        for input in &item.sig.inputs {
            let syn::FnArg::Typed(input) = input else {
                continue;
            };
            let mut symbols = Symbols::default();
            symbols.visit_type(&input.ty);
            if symbols.0.contains("Extension") {
                if symbols.0.contains("AppState") && item.sig.ident != "controller_context" {
                    self.reject(format!(
                        "handler {} extracts application state instead of its controller",
                        item.sig.ident
                    ));
                }
                if symbols.0.iter().any(|name| name.ends_with("Controller")) {
                    self.controller_handlers += 1;
                    let forwards = matches!(item.block.stmts.as_slice(),[syn::Stmt::Expr(syn::Expr::Await(expr),None)] if matches!(expr.base.as_ref(),syn::Expr::MethodCall(call) if matches!(call.receiver.as_ref(),syn::Expr::Path(receiver) if receiver.path.is_ident("controller"))));
                    if !forwards {
                        self.reject(format!(
                            "handler {} does more than forward to its controller",
                            item.sig.ident
                        ));
                    }
                }
            }
        }
        visit::visit_item_fn(self, item);
    }
    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        if test_only(&item.attrs) {
            return;
        }
        if item.ident.to_string().ends_with("Controller")
            || item.ident.to_string().ends_with("Service")
        {
            let mut symbols = Symbols::default();
            symbols.visit_fields(&item.fields);
            for forbidden in [
                "Store",
                "AppState",
                "DatabaseConnection",
                "DatabaseTransaction",
                "sea_orm",
                "entities",
            ] {
                if symbols.0.contains(forbidden) {
                    self.reject(format!("{} owns {forbidden}", item.ident));
                }
            }
        }
        visit::visit_item_struct(self, item);
    }
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        if test_only(&item.attrs) {
            return;
        }
        let old = self.hooks;
        if let Some((_, path, _)) = &item.trait_ {
            if path
                .segments
                .last()
                .is_some_and(|part| part.ident == "CrudLifetime")
            {
                self.hooks = true;
            }
            if path
                .segments
                .last()
                .is_some_and(|part| part.ident == "Repository")
                && let syn::Type::Path(ty) = item.self_ty.as_ref()
                && ty
                    .path
                    .segments
                    .last()
                    .is_none_or(|part| !part.ident.to_string().ends_with("Repository"))
            {
                self.reject("database abstraction does not use a *Repository type");
            }
        }
        if item.trait_.as_ref().is_some_and(|(_, path, _)| {
            path.segments
                .last()
                .is_some_and(|part| part.ident == "CrudResource")
        }) {
            for member in &item.items {
                if let syn::ImplItem::Type(ty) = member
                    && ty.ident == "Repository"
                {
                    let mut symbols = Symbols::default();
                    symbols.visit_type(&ty.ty);
                    if symbols.0.contains("SeaOrmRepo") {
                        self.reject("Dispatch CrudKit mutations bypass domain services");
                    }
                }
            }
        }
        visit::visit_item_impl(self, item);
        self.hooks = old;
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if !test_only(&item.attrs) {
            if item.sig.ident.to_string().ends_with("_in") {
                let mut symbols = Symbols::default();
                symbols.visit_signature(&item.sig);
                if !symbols.0.contains("Transaction") {
                    self.reject(format!(
                        "{} does not require the opaque transaction",
                        item.sig.ident
                    ));
                }
                let mut body = Symbols::default();
                body.visit_block(&item.block);
                if body.0.contains("begin") || body.0.contains("db") {
                    self.reject(format!(
                        "{} reacquires database access within a transaction",
                        item.sig.ident
                    ));
                }
            }
            visit::visit_impl_item_fn(self, item);
        }
    }
    fn visit_ident(&mut self, ident: &'ast syn::Ident) {
        let name = ident.to_string();
        if !self.persistence
            && matches!(
                name.as_str(),
                "sea_orm"
                    | "entities"
                    | "EntityTrait"
                    | "ActiveModelTrait"
                    | "ConnectionTrait"
                    | "DatabaseConnection"
                    | "DatabaseTransaction"
            )
        {
            self.reject(format!("ORM dependency {name} outside persistence"));
        }
        if !self.persistence
            && !self.composition
            && !self.transport
            && matches!(
                name.as_str(),
                "AppState"
                    | "Store"
                    | "expect_context"
                    | "use_context"
                    | "HeaderMap"
                    | "axum"
                    | "leptos"
            )
        {
            self.reject(format!("domain dependency {name} crosses its boundary"));
        }
        if name == "frontend" && !self.composition {
            self.reject("backend depends on frontend rendering");
        }
    }
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let method = call.method.to_string();
        if !self.persistence && !self.composition && matches!(method.as_str(), "connection" | "db")
        {
            self.reject(format!("database access {method} outside persistence"));
        }
        if self.hooks
            && (matches!(
                method.as_str(),
                "create"
                    | "update"
                    | "delete"
                    | "insert"
                    | "save"
                    | "restore"
                    | "detach"
                    | "begin"
                    | "execute"
                    | "exec"
            ) || method.starts_with("publish_"))
        {
            self.reject(format!("CrudKit hook performs {method}"));
        }
        visit::visit_expr_method_call(self, call);
    }
}
fn inspect(directory: &Path, root: &Path, violations: &mut Vec<String>, handlers: &mut usize) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            inspect(&path, root, violations, handlers);
            continue;
        }
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let relative = path.strip_prefix(root).unwrap();
        let name = path.file_name().unwrap().to_str().unwrap();
        if matches!(
            name,
            "tests.rs" | "architecture_tests.rs" | "boundary_tests.rs"
        ) {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap();
        let file = syn::parse_file(&source).unwrap();
        let transport = relative
            .components()
            .any(|part| part.as_os_str() == "transport")
            || matches!(name, "transport.rs" | "api.rs" | "http.rs");
        // CrudKit resource modules implement persistence adapters for generic reads and validation.
        let crud = transport
            && (source.contains("impl Repository<") || source.contains("impl SeaOrmResource for"));
        let persistence =
            crud || relative.components().any(|part| {
                matches!(
                    part.as_os_str().to_str(),
                    Some("repository" | "entities" | "migrations")
                )
            }) || matches!(
                name,
                "repository.rs" | "storage.rs" | "crudkit_resources.rs"
            );
        let composition = relative.components().count() == 1
            && matches!(
                name,
                "application.rs" | "app_state.rs" | "entry.rs" | "server.rs" | "http.rs"
            );
        let mut visitor = Boundaries {
            path: relative,
            persistence,
            composition,
            transport,
            hooks: false,
            violations: vec![],
            controller_handlers: 0,
        };
        visitor.visit_file(&file);
        violations.extend(visitor.violations);
        *handlers += visitor.controller_handlers;
    }
}
#[test]
fn every_backend_domain_obeys_persistence_service_controller_and_hook_boundaries() {
    let backend = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/backend");
    let mut violations = vec![];
    let mut handlers = 0;
    inspect(&backend, &backend, &mut violations, &mut handlers);
    assert_that!(&violations).is_empty();
    assert_that!(&(handlers > 0)).is_true();
}
