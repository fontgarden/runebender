// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The module layering gate.
//!
//! `docs/ARCHITECTURE.md` states an order for the library modules and an order for the
//! application modules. A module may depend on the modules before it in its order. A reference
//! in the other direction is a back edge.
//!
//! This test walks the module tree from `src/lib.rs` and `src/main.rs`, parses each file with
//! `syn`, and counts the references from one module to another. [`LIBRARY_BACK_EDGES`] and
//! [`APPLICATION_BACK_EDGES`] record the permitted number of back edges for each module pair.
//! The test fails when a count is higher than its record, and also when a record is higher than
//! the count. A record can therefore only decrease.
//!
//! What counts as one reference:
//!
//! - each leaf of a `use` declaration;
//! - each path in an item, type, expression or pattern that starts with `crate`, `self`,
//!   `super` or `runebender`;
//! - each such path in the arguments of a macro call.
//!
//! A name that a `use` declaration brought into scope is counted at the declaration only.
//! Items under `#[cfg(test)]` are not production code and are not counted.
//!
//! To move a module in an order, change [`LIBRARY_ORDER`] or [`APPLICATION_ORDER`], run the test,
//! and copy the table that the failure prints. Update the same order in `docs/ARCHITECTURE.md`,
//! which this test compares with the constants.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use syn::visit::Visit as _;

/// Library modules, lowest layer first. Each entry is a child module of `src/lib.rs`.
const LIBRARY_ORDER: &[&str] = &[
    "ui",
    "outline",
    "font",
    "text",
    "formats",
    "analysis",
    "workflows",
    "automation",
];

/// Application modules, lowest layer first. Each entry is a module path below
/// `src/application`. A longer entry takes its files out of a shorter one, so `design::recipes`
/// is a layer of its own and `design` is the remainder of that directory.
const APPLICATION_ORDER: &[&str] = &[
    "design",
    "widgets",
    "design::recipes",
    "local_models",
    "pieces",
    "font_model",
    "workspace",
    "actions",
    "platform",
    "editor",
    "view",
    "launch_options",
    "cli",
    "launch",
    "browser",
];

/// Permitted back edges between library modules: the module that holds the reference, the
/// module that it names, and the number of references.
const LIBRARY_BACK_EDGES: &[(&str, &str, usize)] = &[
    ("outline", "font", 39),
    ("font", "formats", 30),
    ("ui", "workflows", 5),
    ("font", "analysis", 4),
    ("font", "text", 4),
    ("outline", "formats", 4),
    ("ui", "font", 2),
    ("formats", "workflows", 1),
    ("ui", "formats", 1),
    // 9 pairs, 90 references
];

/// Permitted back edges between application modules, in the form of [`LIBRARY_BACK_EDGES`].
const APPLICATION_BACK_EDGES: &[(&str, &str, usize)] = &[

    ("widgets", "workspace", 19),
    ("editor", "view", 16),
    ("platform", "editor", 16),
    ("workspace", "editor", 13),
    ("widgets", "view", 12),
    ("design::recipes", "view", 10),
    ("platform", "view", 9),
    ("workspace", "platform", 8),
    ("widgets", "actions", 7),
    ("actions", "platform", 4),
    ("actions", "editor", 3),
    ("design::recipes", "workspace", 3),
    ("font_model", "editor", 3),
    ("view", "launch_options", 3),
    ("widgets", "editor", 3),
    ("workspace", "view", 3),
    ("design", "platform", 2),
    ("design", "view", 2),
    ("editor", "launch_options", 2),
    ("widgets", "launch_options", 2),
    ("design", "widgets", 1),
    ("editor", "browser", 1),
    ("font_model", "launch_options", 1),
    ("pieces", "workspace", 1),
    ("platform", "browser", 1),
    ("platform", "launch_options", 1),
    ("widgets", "design::recipes", 1),
    ("widgets", "platform", 1),
    ("workspace", "browser", 1),
    // 29 pairs, 149 references
];

/// The document that states the two orders for readers.
const MAP: &str = "docs/ARCHITECTURE.md";

/// The crate that a source file belongs to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    /// The library rooted at `src/lib.rs`.
    Library,
    /// The executable rooted at `src/main.rs`.
    Executable,
}

/// One reference from production code to a module path.
struct Reference {
    /// The file that holds the reference, relative to `src`.
    file: String,
    /// The module that holds the reference, as segments below the crate root.
    from: Vec<String>,
    /// The crate that the named path belongs to.
    to_target: Target,
    /// The named path, as segments below the root of `to_target`.
    to: Vec<String>,
}

/// The result of one walk over a crate.
#[derive(Default)]
struct Scan {
    /// Every production module, as segments below the crate root.
    modules: BTreeSet<Vec<String>>,
    references: Vec<Reference>,
}

struct Visitor<'scan> {
    scan: &'scan mut Scan,
    target: Target,
    /// The file being read.
    file: PathBuf,
    /// The directory that holds the child modules of the current module.
    child_directory: PathBuf,
    /// The current module, as segments below the crate root.
    module: Vec<String>,
}

impl<'ast> syn::visit::Visit<'ast> for Visitor<'_> {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        if item_attrs(item).iter().any(is_test_cfg) {
            return;
        }
        syn::visit::visit_item(self, item);
    }

    fn visit_impl_item(&mut self, item: &'ast syn::ImplItem) {
        if impl_item_attrs(item).iter().any(is_test_cfg) {
            return;
        }
        syn::visit::visit_impl_item(self, item);
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        let name = item.ident.to_string();
        let mounted = path_attribute(&item.attrs);
        if let Some((_, items)) = &item.content {
            // An inline module holds its children in a directory of its own name.
            let previous = self.child_directory.clone();
            self.child_directory = match &mounted {
                Some(path) => self.file.parent().expect("a file has a parent").join(path),
                None => previous.join(&name),
            };
            self.module.push(name);
            self.scan.modules.insert(self.module.clone());
            for item in items {
                self.visit_item(item);
            }
            self.module.pop();
            self.child_directory = previous;
            return;
        }
        let file = match mounted {
            Some(path) => self.file.parent().expect("a file has a parent").join(path),
            None => {
                let flat = self.child_directory.join(format!("{name}.rs"));
                if flat.is_file() {
                    flat
                } else {
                    self.child_directory.join(&name).join("mod.rs")
                }
            }
        };
        let mut module = self.module.clone();
        module.push(name);
        scan_file(self.scan, self.target, &file, module);
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        let mut leaves = Vec::new();
        use_leaves(&item.tree, &mut Vec::new(), &mut leaves);
        for leaf in leaves {
            self.record(&leaf);
        }
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments: Vec<String> = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        self.record(&segments);
        syn::visit::visit_path(self, path);
    }

    // `pub(crate)` and `pub(in crate::font)` name a scope, not a dependency.
    fn visit_visibility(&mut self, _: &'ast syn::Visibility) {}

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        for path in token_paths(&mac.tokens.to_string()) {
            self.record(&path);
        }
    }
}

impl Visitor<'_> {
    /// Records `path` when it names a module of this package.
    fn record(&mut self, path: &[String]) {
        let Some(first) = path.first() else {
            return;
        };
        let (to_target, to) = match first.as_str() {
            "crate" => (self.target, path[1..].to_vec()),
            "runebender" if self.target == Target::Executable => {
                (Target::Library, path[1..].to_vec())
            }
            "self" | "super" => {
                let mut absolute = self.module.clone();
                let mut rest = path;
                while let Some(first) = rest.first() {
                    match first.as_str() {
                        "self" => {}
                        "super" => {
                            absolute.pop();
                        }
                        _ => break,
                    }
                    rest = &rest[1..];
                }
                if rest.is_empty() {
                    return;
                }
                absolute.extend(rest.iter().cloned());
                (self.target, absolute)
            }
            _ => return,
        };
        if to.is_empty() {
            return;
        }
        self.scan.references.push(Reference {
            file: relative(&self.file),
            from: self.module.clone(),
            to_target,
            to,
        });
    }
}

fn source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn relative(file: &Path) -> String {
    file.strip_prefix(source_root())
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Reads one module file and every production module below it.
fn scan_file(scan: &mut Scan, target: Target, file: &Path, module: Vec<String>) {
    let source = std::fs::read_to_string(file).unwrap_or_else(|error| {
        panic!(
            "module `{}` has no file at {}: {error}",
            module.join("::"),
            relative(file)
        )
    });
    let syntax = syn::parse_file(&source)
        .unwrap_or_else(|error| panic!("{} is not valid Rust: {error}", relative(file)));
    if syntax.attrs.iter().any(is_test_cfg) {
        return;
    }
    let directory = file.parent().expect("a file has a parent");
    let stem = file.file_stem().and_then(|stem| stem.to_str());
    // `a/mod.rs`, `lib.rs` and `main.rs` hold their children beside them; `a/b.rs` holds them in
    // `a/b/`.
    let child_directory = match stem {
        Some("mod" | "lib" | "main") => directory.to_path_buf(),
        Some(stem) => directory.join(stem),
        None => directory.to_path_buf(),
    };
    scan.modules.insert(module.clone());
    let mut visitor = Visitor {
        scan,
        target,
        file: file.to_path_buf(),
        child_directory,
        module,
    };
    visitor.visit_file(&syntax);
}

fn scan_crate(target: Target) -> Scan {
    let root = match target {
        Target::Library => "lib.rs",
        Target::Executable => "main.rs",
    };
    let mut scan = Scan::default();
    scan_file(&mut scan, target, &source_root().join(root), Vec::new());
    scan
}

/// The value of `#[path = "..."]`, when the module has one.
fn path_attribute(attrs: &[syn::Attribute]) -> Option<String> {
    attrs.iter().find_map(|attribute| {
        if !attribute.path().is_ident("path") {
            return None;
        }
        let syn::Meta::NameValue(pair) = &attribute.meta else {
            return None;
        };
        let syn::Expr::Lit(literal) = &pair.value else {
            return None;
        };
        let syn::Lit::Str(text) = &literal.lit else {
            return None;
        };
        Some(text.value())
    })
}

/// Every full path that a `use` tree imports.
fn use_leaves(tree: &syn::UseTree, prefix: &mut Vec<String>, leaves: &mut Vec<Vec<String>>) {
    match tree {
        syn::UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            use_leaves(&path.tree, prefix, leaves);
            prefix.pop();
        }
        syn::UseTree::Name(name) => {
            let mut leaf = prefix.clone();
            // `use a::b::{self}` imports `a::b`.
            if name.ident != "self" || leaf.is_empty() {
                leaf.push(name.ident.to_string());
            }
            leaves.push(leaf);
        }
        syn::UseTree::Rename(rename) => {
            let mut leaf = prefix.clone();
            if rename.ident != "self" || leaf.is_empty() {
                leaf.push(rename.ident.to_string());
            }
            leaves.push(leaf);
        }
        syn::UseTree::Glob(_) => leaves.push(prefix.clone()),
        syn::UseTree::Group(group) => {
            for tree in &group.items {
                use_leaves(tree, prefix, leaves);
            }
        }
    }
}

/// Every path in a macro's tokens that starts with `crate`, `self`, `super` or `runebender`.
///
/// `syn` does not parse macro arguments, so this reads the printed tokens. String literals are
/// skipped.
fn token_paths(tokens: &str) -> Vec<Vec<String>> {
    #[derive(PartialEq)]
    enum Token {
        Ident(String),
        Separator,
        Other,
    }
    let mut stream = Vec::new();
    let mut characters = tokens.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '"' {
            while let Some(inner) = characters.next() {
                match inner {
                    '\\' => {
                        characters.next();
                    }
                    '"' => break,
                    _ => {}
                }
            }
            stream.push(Token::Other);
        } else if character.is_alphanumeric() || character == '_' {
            let mut ident = String::from(character);
            while let Some(next) = characters.peek().copied() {
                if next.is_alphanumeric() || next == '_' {
                    ident.push(next);
                    characters.next();
                } else {
                    break;
                }
            }
            stream.push(Token::Ident(ident));
        } else if character == ':' && characters.peek() == Some(&':') {
            characters.next();
            stream.push(Token::Separator);
        } else if !character.is_whitespace() {
            stream.push(Token::Other);
        }
    }
    let mut paths = Vec::new();
    let mut index = 0;
    while index < stream.len() {
        let starts_path = matches!(
            &stream[index],
            Token::Ident(ident)
                if matches!(ident.as_str(), "crate" | "self" | "super" | "runebender")
        ) && (index == 0 || stream[index - 1] != Token::Separator);
        if !starts_path {
            index += 1;
            continue;
        }
        let mut path = Vec::new();
        while let Some(Token::Ident(ident)) = stream.get(index) {
            path.push(ident.clone());
            if stream.get(index + 1) != Some(&Token::Separator) {
                index += 1;
                break;
            }
            index += 2;
        }
        if path.len() > 1 {
            paths.push(path);
        }
    }
    paths
}

fn item_attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Const(item) => &item.attrs,
        syn::Item::Enum(item) => &item.attrs,
        syn::Item::ExternCrate(item) => &item.attrs,
        syn::Item::Fn(item) => &item.attrs,
        syn::Item::ForeignMod(item) => &item.attrs,
        syn::Item::Impl(item) => &item.attrs,
        syn::Item::Macro(item) => &item.attrs,
        syn::Item::Mod(item) => &item.attrs,
        syn::Item::Static(item) => &item.attrs,
        syn::Item::Struct(item) => &item.attrs,
        syn::Item::Trait(item) => &item.attrs,
        syn::Item::TraitAlias(item) => &item.attrs,
        syn::Item::Type(item) => &item.attrs,
        syn::Item::Union(item) => &item.attrs,
        syn::Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

fn impl_item_attrs(item: &syn::ImplItem) -> &[syn::Attribute] {
    match item {
        syn::ImplItem::Const(item) => &item.attrs,
        syn::ImplItem::Fn(item) => &item.attrs,
        syn::ImplItem::Type(item) => &item.attrs,
        syn::ImplItem::Macro(item) => &item.attrs,
        _ => &[],
    }
}

/// True for `#[cfg(test)]` and for `#[cfg(all(test, ...))]`.
///
/// `#[cfg(any(test, ...))]` also compiles in production, so it is not a test attribute.
fn is_test_cfg(attribute: &syn::Attribute) -> bool {
    if !attribute.path().is_ident("cfg") {
        return false;
    }
    let Ok(list) = attribute.meta.require_list() else {
        return false;
    };
    let tokens: String = list
        .tokens
        .to_string()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    tokens == "test" || tokens.starts_with("all(test,")
}

/// The entry of `order` that owns `path`: the longest entry that is a prefix of it.
fn layer_of(order: &[&'static str], path: &[String]) -> Option<&'static str> {
    order
        .iter()
        .copied()
        .filter(|entry| {
            let segments: Vec<&str> = entry.split("::").collect();
            path.len() >= segments.len()
                && segments
                    .iter()
                    .zip(path)
                    .all(|(segment, part)| segment == part)
        })
        .max_by_key(|entry| entry.len())
}

/// The references between the layers of one order.
struct Edges {
    /// For each pair of different layers, the file and the named path of every reference.
    by_pair: BTreeMap<(&'static str, &'static str), Vec<(String, String)>>,
    /// Faults in the order itself.
    faults: Vec<String>,
}

/// Groups the references of `scan` by layer pair.
///
/// `base` holds the segments between the crate root and the modules that `order` names. It is
/// empty for the library and `["application"]` for the executable.
fn edges(scan: &Scan, target: Target, base: &[&str], order: &[&'static str], name: &str) -> Edges {
    let below_base = |path: &[String]| -> Option<Vec<String>> {
        (path.len() > base.len() && base.iter().zip(path).all(|(segment, part)| segment == part))
            .then(|| path[base.len()..].to_vec())
    };
    let mut faults = Vec::new();
    for entry in order {
        let segments: Vec<String> = base
            .iter()
            .copied()
            .chain(entry.split("::"))
            .map(str::to_owned)
            .collect();
        if !scan.modules.contains(&segments) {
            faults.push(format!(
                "{name} names `{entry}`, but no production module has that path. \
                 Rename or remove the entry."
            ));
        }
    }
    let unplaced: BTreeSet<&str> = scan
        .modules
        .iter()
        .filter(|module| module.len() == base.len() + 1)
        .filter(|module| below_base(module).is_some())
        .map(|module| &module[base.len()])
        .filter(|module| layer_of(order, std::slice::from_ref(*module)).is_none())
        .map(String::as_str)
        .collect();
    for module in unplaced {
        faults.push(format!(
            "module `{module}` has no place in {name}. Add it to the order."
        ));
    }
    let mut by_pair: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for reference in &scan.references {
        if reference.to_target != target {
            continue;
        }
        let (Some(from), Some(to)) = (below_base(&reference.from), below_base(&reference.to))
        else {
            continue;
        };
        let (Some(from), Some(to)) = (layer_of(order, &from), layer_of(order, &to)) else {
            continue;
        };
        if from != to {
            by_pair
                .entry((from, to))
                .or_default()
                .push((reference.file.clone(), reference.to.join("::")));
        }
    }
    Edges { by_pair, faults }
}

fn rank(order: &[&str], layer: &str) -> usize {
    order
        .iter()
        .position(|entry| *entry == layer)
        .expect("a layer comes from the order")
}

/// The back edges of `edges`, largest first.
fn back_edges(edges: &Edges, order: &[&str]) -> Vec<(&'static str, &'static str, usize)> {
    let mut rows: Vec<_> = edges
        .by_pair
        .iter()
        .filter(|((from, to), _)| rank(order, from) < rank(order, to))
        .map(|((from, to), references)| (*from, *to, references.len()))
        .collect();
    rows.sort_by(|a, b| b.2.cmp(&a.2).then(a.cmp(b)));
    rows
}

/// The table as Rust source, ready to paste over the recorded one.
fn table_source(rows: &[(&str, &str, usize)]) -> String {
    let mut text = String::new();
    for (from, to, count) in rows {
        writeln!(text, "    (\"{from}\", \"{to}\", {count}),").expect("write to a string");
    }
    let total: usize = rows.iter().map(|row| row.2).sum();
    writeln!(text, "    // {} pairs, {total} references", rows.len()).expect("write to a string");
    text
}

/// The references of one pair, grouped by file and by named path.
fn pair_detail(edges: &Edges, from: &'static str, to: &'static str) -> String {
    let mut counts: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for (file, path) in edges.by_pair.get(&(from, to)).into_iter().flatten() {
        *counts.entry((file, path)).or_default() += 1;
    }
    let mut text = String::new();
    for ((file, path), count) in counts {
        writeln!(text, "      {file}: {path} ({count})").expect("write to a string");
    }
    text
}

fn check(
    name: &str,
    table: &str,
    target: Target,
    base: &[&str],
    order: &[&'static str],
    recorded: &[(&'static str, &'static str, usize)],
) {
    let scan = scan_crate(target);
    let edges = edges(&scan, target, base, order, name);
    let mut faults = edges.faults.clone();
    let actual = back_edges(&edges, order);
    for (from, to, _) in recorded {
        for layer in [from, to] {
            if !order.contains(layer) {
                faults.push(format!(
                    "{table} has the row (\"{from}\", \"{to}\"), but `{layer}` is not in {name}."
                ));
            }
        }
    }
    if faults.is_empty() {
        let recorded_count = |from: &str, to: &str| {
            recorded
                .iter()
                .find(|row| row.0 == from && row.1 == to)
                .map_or(0, |row| row.2)
        };
        for &(from, to, count) in &actual {
            let permitted = recorded_count(from, to);
            if count > permitted {
                faults.push(format!(
                    "`{from}` holds {count} reference(s) to `{to}`; {table} permits {permitted}. \
                     `{from}` is below `{to}`, so remove the new reference.\n{}",
                    pair_detail(&edges, from, to)
                ));
            } else if count < permitted {
                faults.push(format!(
                    "`{from}` holds {count} reference(s) to `{to}`; {table} records {permitted}. \
                     Lower the row to {count}."
                ));
            }
        }
        for &(from, to, permitted) in recorded {
            if rank(order, from) > rank(order, to) {
                faults.push(format!(
                    "{table} has the row (\"{from}\", \"{to}\"), but `{from}` is above `{to}` \
                     in {name}. Remove the row."
                ));
            } else if !actual.iter().any(|row| row.0 == from && row.1 == to) {
                faults.push(format!(
                    "`{from}` holds no reference to `{to}`; {table} records {permitted}. \
                     Remove the row."
                ));
            }
        }
    }
    assert!(
        faults.is_empty(),
        "tests/module_layering.rs: {name} and the code disagree.\n\n{}\n\n\
         The current table for {table}:\n\n{}",
        faults.join("\n"),
        table_source(&actual)
    );
}

#[test]
fn library_back_edges_do_not_increase() {
    check(
        "LIBRARY_ORDER",
        "LIBRARY_BACK_EDGES",
        Target::Library,
        &[],
        LIBRARY_ORDER,
        LIBRARY_BACK_EDGES,
    );
}

#[test]
fn application_back_edges_do_not_increase() {
    check(
        "APPLICATION_ORDER",
        "APPLICATION_BACK_EDGES",
        Target::Executable,
        &["application"],
        APPLICATION_ORDER,
        APPLICATION_BACK_EDGES,
    );
}

/// Prints every back edge with the files and paths that cause it.
///
/// Run `cargo test --test module_layering -- --ignored --nocapture` to choose the next move.
#[test]
#[ignore = "prints a report and checks nothing"]
fn print_back_edge_references() {
    for (name, target, base, order) in [
        ("Library", Target::Library, &[][..], LIBRARY_ORDER),
        (
            "Application",
            Target::Executable,
            &["application"][..],
            APPLICATION_ORDER,
        ),
    ] {
        let scan = scan_crate(target);
        let edges = edges(&scan, target, base, order, name);
        println!("{name} edges");
        for ((from, to), references) in &edges.by_pair {
            let back = if rank(order, from) < rank(order, to) {
                " (back edge)"
            } else {
                ""
            };
            println!("  {from} -> {to}: {}{back}", references.len());
        }
        println!("{name} back edges");
        for (from, to, count) in back_edges(&edges, order) {
            println!("  {from} -> {to}: {count}");
            print!("{}", pair_detail(&edges, from, to));
        }
    }
}

/// The names in backticks between `<!-- check: name -->` and the next `<!-- end -->` of the
/// document.
fn documented_order(name: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(MAP);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let start = format!("<!-- check: {name} -->");
    let after = text
        .split_once(&start)
        .unwrap_or_else(|| panic!("{MAP} has no `{start}` block"))
        .1;
    let block = after
        .split_once("<!-- end -->")
        .unwrap_or_else(|| panic!("{MAP}: `{start}` has no `<!-- end -->`"))
        .0;
    block
        .split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_document_states_the_same_orders() {
    for (name, order) in [
        ("library-order", LIBRARY_ORDER),
        ("application-order", APPLICATION_ORDER),
    ] {
        assert_eq!(
            documented_order(name),
            order,
            "{MAP} `{name}` and tests/module_layering.rs state different orders. \
             Make the document match the constant."
        );
    }
}

#[test]
fn macro_arguments_are_read() {
    let paths =
        token_paths(r#""crate::not::this" , crate :: font :: Project , (super :: a) self . b"#);
    assert_eq!(
        paths,
        vec![
            vec!["crate".to_owned(), "font".to_owned(), "Project".to_owned()],
            vec!["super".to_owned(), "a".to_owned()],
        ]
    );
}
