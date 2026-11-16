//! The capability inventory rule: one row per registered command, and one label and key per id.
//!
//! ADR 0117 makes a const table the register of the interface: a command that no surface draws is a
//! finding, and a command with two homes that disagree is the same defect twice. `xtask` cannot link
//! `ui` — it is the build tool, not the product — so this check reads the sources the way
//! `ui_rules` does, with `syn`, and resolves the two registers by name:
//!
//! - the registry (`static COMMANDS: &[CommandSpec]`) names `panel_cmds::CREATE`;
//! - that const in `crates/engine/src/panel_cmds/` is `CommandSpec::new(CREATE_ID, CREATE_LABEL, …)`;
//! - `CREATE_ID` in the same file is the string the user never types, and `CREATE_LABEL` is the
//!   message the interface prints.
//!
//! A command missing from the inventory, a row naming an id that is neither a command nor a declared
//! atom, an `Api` row that also claims a surface, a second row for one id, two rows sharing a key, and
//! a live ribbon cell with no artwork all fail the gate. One icon file shared by two commands is
//! printed for review and passes, because a service action may legitimately reuse one drawing.
//!
//! Test code is not read: a `#[cfg(test)]` item is skipped, exactly as in `ui_rules`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use syn::visit::{self, Visit};
use syn::{Expr, Item, Lit};

/// The capability inventory and the sources it must agree with.
const INVENTORY: &str = "crates/ui/src/inventory.rs";
const REGISTRY: &str = "crates/engine/src/commands.rs";
const REGISTRY_STATIC: &str = "COMMANDS";
const ENGINE_TREE: &str = "crates/engine/src";
const RIBBON_MAP: &str = "crates/ui/src/toolbars/spec.rs";

/// How deep the module walk goes. The engine tree is three levels; a runaway link is not a task.
const MAX_DEPTH: usize = 8;

/// One row of the inventory, as the gate reads it from the source.
struct Row {
    id: String,
    label: String,
    places: Vec<String>,
    icon: Option<String>,
    line: usize,
}

/// One key binding, as the gate reads it from the key table (U96).
struct Binding {
    id: String,
    key: String,
    chord: String,
    label: String,
    line: usize,
}

/// One registered command, as the registry names it.
struct Command {
    what: String,
    label: String,
}

/// Checks the inventory against the registry and the ribbon map; returns how many files it read.
///
/// `None` means the tree has no inventory (a scratch tree in a unit test), so the rule has nothing to
/// say about it.
pub fn check(root: &Path, violations: &mut Vec<String>) -> Option<usize> {
    let inventory_source = std::fs::read_to_string(root.join(INVENTORY)).ok()?;
    let mut read = 1usize;

    let rows = match rows(&inventory_source) {
        Ok(rows) => rows,
        Err(error) => {
            violations.push(error);
            return Some(read);
        }
    };
    let atoms = atoms(&inventory_source).unwrap_or_default();
    let keys = match bindings(&inventory_source) {
        Ok(keys) => keys,
        Err(error) => {
            violations.push(error);
            Vec::new()
        }
    };
    let actions = actions(&inventory_source).unwrap_or_default();

    let commands = match registered(root) {
        Ok(commands) => {
            read = read.saturating_add(1);
            commands
        }
        Err(error) => {
            violations.push(error);
            BTreeMap::new()
        }
    };

    check_rows(&rows, &atoms, &commands, violations);
    coverage(&rows, &commands, violations);
    if let Ok(source) = std::fs::read_to_string(root.join(RIBBON_MAP)) {
        read = read.saturating_add(1);
        let methods = check_ribbon(&source, violations);
        check_keys(&keys, &actions, &commands, &atoms, &methods, violations);
    }
    Some(read)
}

/// The rules that read the key table: one key per id, one id per key, and an id the interface knows.
///
/// A binding names a command, an atom, a shell action or a ribbon method, and nothing else: a typo in
/// an id would otherwise be a key the shell reads and nothing runs (U96).
fn check_keys(
    keys: &[Binding],
    actions: &BTreeSet<String>,
    commands: &BTreeMap<String, Command>,
    atoms: &BTreeSet<String>,
    methods: &BTreeSet<String>,
    violations: &mut Vec<String>,
) {
    let mut pairs: BTreeMap<(&str, &str), &str> = BTreeMap::new();
    let mut ids: BTreeMap<&str, usize> = BTreeMap::new();
    let mut labels: BTreeMap<&str, &str> = BTreeMap::new();
    for binding in keys {
        if let Some(first) = ids.insert(&binding.id, binding.line) {
            violations.push(format!(
                "`{INVENTORY}:{}` is the second key for `{}`, which already has one at line {first}; one \
                 binding runs one id (ADR 0117)",
                binding.line, binding.id
            ));
        }
        if let Some(other) = pairs.insert((&binding.chord, &binding.key), &binding.id) {
            violations.push(format!(
                "`{INVENTORY}:{}` binds `{}` to `{}`, and `{other}` already carries it; one key runs one id \
                 (ADR 0117)",
                binding.line, binding.label, binding.id
            ));
        }
        if let Some(other) = labels.insert(&binding.label, &binding.id) {
            violations.push(format!(
                "`{INVENTORY}:{}` prints `{}` for `{}`, and `{other}` already prints it; one key has one \
                 spelling (ADR 0117)",
                binding.line, binding.label, binding.id
            ));
        }
        let known = commands.contains_key(&binding.id)
            || atoms.contains(&binding.id)
            || actions.contains(&binding.id)
            || methods.contains(&binding.id);
        if !known {
            violations.push(format!(
                "`{INVENTORY}:{}` binds `{}` to `{}`, which is no registered command, no atom, no shell \
                 action and no method of `{RIBBON_MAP}`",
                binding.line, binding.label, binding.id
            ));
        }
    }
    for action in actions {
        if !ids.contains_key(action.as_str()) {
            violations.push(format!(
                "the shell action `{action}` of `{INVENTORY}` has no key; an action no key runs is a row no \
                 surface reads (ADR 0117)"
            ));
        }
        if commands.contains_key(action) {
            violations.push(format!(
                "`{action}` is declared both a shell action and a command of `crates/engine/src/commands.rs`; \
                 a command's key hangs off its own id (ADR 0117)"
            ));
        }
    }
}

/// The rules that read the inventory alone: known ids, one row per id, one label each.
fn check_rows(
    rows: &[Row],
    atoms: &BTreeSet<String>,
    commands: &BTreeMap<String, Command>,
    violations: &mut Vec<String>,
) {
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    let mut icons: BTreeMap<&str, &str> = BTreeMap::new();

    for row in rows {
        if let Some(first) = seen.insert(&row.id, row.line) {
            violations.push(format!(
                "`{INVENTORY}:{}` is the second row for `{}`, which already has one at line {first}; \
                 one capability is one row (ADR 0117)",
                row.line, row.id
            ));
        }
        if !commands.contains_key(&row.id) && !atoms.contains(&row.id) {
            violations.push(format!(
                "`{INVENTORY}:{}` has a row for `{}`, which is no registered command of \
                 `crates/engine/src/commands.rs` and no atom declared there (ADR 0117)",
                row.line, row.id
            ));
        }
        if row.places.iter().any(|place| place == "Api") && row.places.len() > 1 {
            violations.push(format!(
                "`{INVENTORY}:{}` puts `{}` in the API and on a surface; `Api` is the one place that \
                 means no surface draws it (ADR 0117)",
                row.line, row.id
            ));
        }
        if let Some(icon) = row.icon.as_deref()
            && let Some(other) = icons.insert(icon, &row.id)
        {
            println!("ui-rules: note: `{icon}` draws both `{other}` and `{}`, for review", row.id);
        }
        if let Some(command) = commands.get(&row.id)
            && command.label != row.label
        {
            violations.push(format!(
                "`{INVENTORY}:{}` labels `{}` with `{}`, and `{}` labels it `{}`; the words come from \
                 the command, not from the table (ADR 0117)",
                row.line, row.id, row.label, command.what, command.label
            ));
        }
    }
}

/// Every registered command has a row.
fn coverage(rows: &[Row], commands: &BTreeMap<String, Command>, violations: &mut Vec<String>) {
    let known: BTreeSet<&str> = rows.iter().map(|row| row.id.as_str()).collect();
    for (id, command) in commands {
        if known.contains(id.as_str()) {
            continue;
        }
        violations.push(format!(
            "the command `{id}` (`{}`) has no row in `{INVENTORY}`; every command has a place, or \
             declares `Api` with the reason (ADR 0117)",
            command.what
        ));
    }
}

/// A live ribbon cell is drawn, so it names its artwork.
fn check_ribbon(source: &str, violations: &mut Vec<String>) -> BTreeSet<String> {
    let Ok(file) = syn::parse_file(source) else {
        violations.push(format!("`{RIBBON_MAP}` does not parse as Rust"));
        return BTreeSet::new();
    };
    let mut scan = Ribbon { violations, methods: BTreeSet::new() };
    scan.visit_file(&file);
    scan.methods
}

struct Ribbon<'a> {
    violations: &'a mut Vec<String>,
    /// Every method id the map declares: a drawing key is filed under one of these (U96).
    methods: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for Ribbon<'_> {
    fn visit_item(&mut self, item: &'ast Item) {
        if test_item(item) {
            return;
        }
        visit::visit_item(self, item);
    }

    /// A planned cell is built by `planned(id, label)`, a call rather than a literal, and a key may
    /// already be filed under it: the id counts as a method of the map all the same (U96).
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let Some(path) = path_text(&call.func)
            && last_segment(&path) == "planned"
            && let Some(id) = call.args.first().and_then(string_literal)
        {
            self.methods.insert(id);
        }
        visit::visit_expr_call(self, call);
    }

    fn visit_expr_struct(&mut self, expression: &'ast syn::ExprStruct) {
        let path = path_text_from(&expression.path);
        let tool = path.ends_with("ToolSpec");
        let variant = path.ends_with("VariantSpec");
        if tool || variant {
            let id = field(expression, "id").and_then(string_literal).unwrap_or_else(|| "unnamed".to_owned());
            self.methods.insert(id.clone());
            let live = field(expression, "run").is_some_and(|run| matches!(run, Expr::Call(_)));
            let drawn = field(expression, "icon").is_some_and(|icon| matches!(icon, Expr::Call(_)));
            if live && !drawn {
                let kind = if tool { "cell" } else { "method" };
                self.violations.push(format!(
                    "`{RIBBON_MAP}:{}` declares the live {kind} `{id}` with no artwork; a cell that \
                     runs is drawn, so it names an icon file (ADR 0117)",
                    expression.brace_token.span.open().start().line
                ));
            }
        }
        visit::visit_expr_struct(self, expression);
    }
}

/// Whether an item is declared under `#[cfg(test)]`, which is not shipped code.
fn test_item(item: &Item) -> bool {
    match item {
        Item::Const(item) => test_attributes(&item.attrs),
        Item::Enum(item) => test_attributes(&item.attrs),
        Item::Fn(item) => test_attributes(&item.attrs),
        Item::Impl(item) => test_attributes(&item.attrs),
        Item::Mod(item) => test_attributes(&item.attrs),
        Item::Static(item) => test_attributes(&item.attrs),
        Item::Struct(item) => test_attributes(&item.attrs),
        Item::Trait(item) => test_attributes(&item.attrs),
        Item::Type(item) => test_attributes(&item.attrs),
        Item::Use(item) => test_attributes(&item.attrs),
        _ => false,
    }
}

fn test_attributes(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        if !attribute.path().is_ident("cfg") {
            return false;
        }
        match &attribute.meta {
            syn::Meta::List(list) => list.tokens.to_string().contains("test"),
            _ => false,
        }
    })
}

fn field<'a>(expression: &'a syn::ExprStruct, name: &str) -> Option<&'a Expr> {
    expression.fields.iter().find_map(|member| match &member.member {
        syn::Member::Named(ident) if ident == name => Some(&member.expr),
        _ => None,
    })
}

fn string_literal(expression: &Expr) -> Option<String> {
    match expression {
        Expr::Lit(literal) => match &literal.lit {
            Lit::Str(text) => Some(text.value()),
            _ => None,
        },
        Expr::Group(group) => string_literal(&group.expr),
        Expr::Paren(paren) => string_literal(&paren.expr),
        _ => None,
    }
}

fn path_text_from(path: &syn::Path) -> String {
    path.segments.iter().map(|segment| segment.ident.to_string()).collect::<Vec<_>>().join("::")
}

fn path_text(expression: &Expr) -> Option<String> {
    match expression {
        Expr::Path(path) => Some(path_text_from(&path.path)),
        Expr::Group(group) => path_text(&group.expr),
        Expr::Paren(paren) => path_text(&paren.expr),
        _ => None,
    }
}

/// The value inside `Some(..)`, or `None` for an expression that is not a `Some`.
fn some_value(expression: &Expr) -> Option<&Expr> {
    let Expr::Call(call) = expression else { return None };
    if path_text(&call.func)?.ends_with("Some") { call.args.first() } else { None }
}

/// The elements of an array expression, seen through a borrow.
fn array(expression: &Expr) -> Option<Vec<&Expr>> {
    match expression {
        Expr::Array(array) => Some(array.elems.iter().collect()),
        Expr::Reference(reference) => array(&reference.expr),
        Expr::Group(group) => array(&group.expr),
        Expr::Paren(paren) => array(&paren.expr),
        _ => None,
    }
}

/// The rows of `const ROWS: &[Row] = &[…];`.
fn rows(source: &str) -> Result<Vec<Row>, String> {
    let file = syn::parse_file(source).map_err(|error| format!("`{INVENTORY}` does not parse as Rust: {error}"))?;
    let table = file.items.iter().find_map(|item| match item {
        Item::Const(item) if item.ident == "ROWS" => Some(&*item.expr),
        _ => None,
    });
    let Some(table) = table else {
        return Err(format!("`{INVENTORY}` has no `const ROWS: &[Row]`; the gate reads nothing else"));
    };
    let Some(elements) = array(table) else {
        return Err(format!("`{INVENTORY}`: `ROWS` is not an array of rows"));
    };
    let mut rows = Vec::new();
    for element in elements {
        let Expr::Struct(row) = element else { continue };
        let Some(id) = field(row, "id").and_then(string_literal) else {
            return Err(format!("`{INVENTORY}`: a row has no id"));
        };
        let label = field(row, "label").and_then(path_text).unwrap_or_default();
        let Some(place_list) = field(row, "places") else {
            return Err(format!("`{INVENTORY}`: the row `{id}` has no places"));
        };
        let places = array(place_list).unwrap_or_default();
        let places: Vec<String> =
            places.iter().filter_map(|place| path_text(place)).map(|place| last_segment(&place).to_owned()).collect();
        rows.push(Row {
            id,
            label,
            places,
            icon: field(row, "icon").and_then(some_value).and_then(path_text),
            line: row.brace_token.span.open().start().line,
        });
    }
    Ok(rows)
}

/// The string constants of the inventory, so a binding may name its id as `FILE_SAVE` (U96).
fn string_consts(file: &syn::File) -> BTreeMap<String, String> {
    file.items
        .iter()
        .filter_map(|item| match item {
            Item::Const(item) => string_literal(&item.expr).map(|value| (item.ident.to_string(), value)),
            _ => None,
        })
        .collect()
}

/// The id a binding or an action names: a literal, or the const the row points at.
fn table_id(expression: &Expr, consts: &BTreeMap<String, String>) -> Option<String> {
    if let Some(text) = string_literal(expression) {
        return Some(text);
    }
    let path = path_text(expression)?;
    consts.get(last_segment(&path)).cloned()
}

/// The key bindings of `const KEYS: &[Binding] = &[…];` (U96).
fn bindings(source: &str) -> Result<Vec<Binding>, String> {
    let file = syn::parse_file(source).map_err(|error| format!("`{INVENTORY}` does not parse as Rust: {error}"))?;
    let consts = string_consts(&file);
    let table = file.items.iter().find_map(|item| match item {
        Item::Const(item) if item.ident == "KEYS" => Some(&*item.expr),
        _ => None,
    });
    let Some(table) = table else {
        return Err(format!("`{INVENTORY}` has no `const KEYS: &[Binding]`; the keymap rule reads nothing else"));
    };
    let Some(elements) = array(table) else {
        return Err(format!("`{INVENTORY}`: `KEYS` is not an array of bindings"));
    };
    let mut keys = Vec::new();
    for element in elements {
        let Expr::Struct(binding) = element else { continue };
        let Some(id) = field(binding, "id").and_then(|id| table_id(id, &consts)) else {
            return Err(format!("`{INVENTORY}`: a key binding has no id"));
        };
        let key = field(binding, "key")
            .and_then(path_text)
            .map(|path| last_segment(&path).to_owned())
            .ok_or_else(|| format!("`{INVENTORY}`: the binding of `{id}` names no key"))?;
        let chord = field(binding, "chord")
            .and_then(path_text)
            .map(|path| last_segment(&path).to_owned())
            .ok_or_else(|| format!("`{INVENTORY}`: the binding of `{id}` names no chord"))?;
        let label = field(binding, "label").and_then(string_literal).unwrap_or_default();
        keys.push(Binding { id, key, chord, label, line: binding.brace_token.span.open().start().line });
    }
    Ok(keys)
}

/// The shell action ids of `const ACTIONS: &[Action] = &[…];` (U96).
fn actions(source: &str) -> Option<BTreeSet<String>> {
    let file = syn::parse_file(source).ok()?;
    let consts = string_consts(&file);
    let table = file.items.iter().find_map(|item| match item {
        Item::Const(item) if item.ident == "ACTIONS" => Some(&*item.expr),
        _ => None,
    })?;
    Some(
        array(table)?
            .iter()
            .filter_map(|element| match element {
                Expr::Struct(action) => field(action, "id").and_then(|id| table_id(id, &consts)),
                other => table_id(other, &consts),
            })
            .collect(),
    )
}

/// The atom ids the inventory may key a row by, `const ATOMS: &[&str] = &[…];` (ADR 0118).
fn atoms(source: &str) -> Option<BTreeSet<String>> {
    let file = syn::parse_file(source).ok()?;
    let table = file.items.iter().find_map(|item| match item {
        Item::Const(item) if item.ident == "ATOMS" => Some(&*item.expr),
        _ => None,
    })?;
    Some(array(table)?.iter().filter_map(|element| string_literal(element)).collect())
}

fn last_segment(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path)
}

/// Every command the engine registers: its id, the const that defines it and the message it prints.
fn registered(root: &Path) -> Result<BTreeMap<String, Command>, String> {
    let path = root.join(REGISTRY);
    let source = std::fs::read_to_string(&path).map_err(|error| format!("cannot read `{REGISTRY}`: {error}"))?;
    let file = syn::parse_file(&source).map_err(|error| format!("`{REGISTRY}` does not parse as Rust: {error}"))?;
    let mut commands = BTreeMap::new();
    for item in &file.items {
        let Item::Static(item) = item else { continue };
        if item.ident != REGISTRY_STATIC {
            continue;
        }
        let Some(entries) = array(&item.expr) else {
            return Err(format!("`{REGISTRY}`: `{REGISTRY_STATIC}` is not an array of specs"));
        };
        for entry in entries {
            let Some(path) = path_text(entry) else { continue };
            let Some((module, name)) = path.split_once("::") else {
                return Err(format!("`{REGISTRY}`: `{path}` does not name a `module::CONST`"));
            };
            let (id, label) = resolve(root, module, name)?;
            commands.insert(id, Command { what: path, label });
        }
    }
    if commands.is_empty() {
        return Err(format!("`{REGISTRY}` names no command in `{REGISTRY_STATIC}`"));
    }
    Ok(commands)
}

/// Resolves one `module::CONST` registry entry to the id and label its `CommandSpec::new` names.
fn resolve(root: &Path, module: &str, name: &str) -> Result<(String, String), String> {
    for file in module_files(root, module)? {
        let shown = file.to_string_lossy().replace('\\', "/");
        let Ok(source) = std::fs::read_to_string(&file) else { continue };
        let Ok(parsed) = syn::parse_file(&source) else { continue };
        let Some((id_const, label_const)) = spec(&parsed, name) else { continue };
        let Some(id) = const_string(&parsed, &id_const) else {
            return Err(format!("`{shown}`: `{id_const}` is not a `&str` const"));
        };
        // The spec names its label const; the row has to agree with the message that const holds.
        let label = const_path(&parsed, &label_const).unwrap_or(label_const);
        return Ok((id, label));
    }
    Err(format!(
        "`{REGISTRY}` names `{module}::{name}`, and `{ENGINE_TREE}/{module}` does not define it as a \
         `CommandSpec::new(<ID>_ID, <LABEL>, …)` const"
    ))
}

/// The `(id const, label path)` a spec const names.
fn spec(file: &syn::File, name: &str) -> Option<(String, String)> {
    for item in &file.items {
        let Item::Const(item) = item else { continue };
        if item.ident != name || test_attributes(&item.attrs) {
            continue;
        }
        let Some(call) = spec_call(&item.expr) else { continue };
        let mut arguments = call.args.iter();
        let id = last_segment(&arguments.next().and_then(path_text)?).to_owned();
        let label = arguments.next().and_then(path_text)?;
        return Some((id, label));
    }
    None
}

/// The `CommandSpec::new(..)` a spec const builds, behind the `.creating(..)` a command may chain.
fn spec_call(expression: &Expr) -> Option<&syn::ExprCall> {
    match expression {
        Expr::Call(call) => {
            let path = path_text(&call.func)?;
            path.ends_with("CommandSpec::new").then_some(call)
        }
        Expr::MethodCall(call) => spec_call(&call.receiver),
        Expr::Reference(reference) => spec_call(&reference.expr),
        Expr::Group(group) => spec_call(&group.expr),
        Expr::Paren(paren) => spec_call(&paren.expr),
        _ => None,
    }
}

/// The path value of a const, by name — the `Message::X` behind `CREATE_LABEL`.
fn const_path(file: &syn::File, name: &str) -> Option<String> {
    for item in &file.items {
        let Item::Const(item) = item else { continue };
        if item.ident == name {
            return path_text(&item.expr);
        }
    }
    None
}

/// A `&'static str` const's value, by name.
fn const_string(file: &syn::File, name: &str) -> Option<String> {
    for item in &file.items {
        let Item::Const(item) = item else { continue };
        if item.ident == name {
            return string_literal(&item.expr);
        }
    }
    None
}

/// The files one engine module is defined in: `<module>.rs`, or every `.rs` under `<module>/`.
fn module_files(root: &Path, module: &str) -> Result<Vec<PathBuf>, String> {
    let directory = root.join(ENGINE_TREE);
    let direct = directory.join(format!("{module}.rs"));
    if direct.is_file() {
        return Ok(vec![direct]);
    }
    let mut files = Vec::new();
    walk(&directory.join(module), 0, &mut files).map_err(|error| format!("`{ENGINE_TREE}/{module}`: {error}"))?;
    files.sort();
    Ok(files)
}

/// Every `.rs` file under `directory`, to [`MAX_DEPTH`].
fn walk(directory: &Path, depth: usize, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err(format!("the walk under `{}` is deeper than {MAX_DEPTH}", directory.display()));
    }
    let entries =
        std::fs::read_dir(directory).map_err(|error| format!("cannot read `{}`: {error}", directory.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, depth.saturating_add(1), files)?;
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            files.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGISTRY: &str = "static COMMANDS: &[CommandSpec] = &[\n    foo_cmds::BAR,\n];\n";
    const FOO: &str = "pub const BAR_ID: &str = \"foo.bar\";\npub const BAR_LABEL: Message = Message::FooBar;\npub(crate) const BAR: CommandSpec = CommandSpec::new(BAR_ID, BAR_LABEL, enabled, run);\n";

    fn row(id: &str, places: &str, icon: &str) -> String {
        format!(
            "Row {{\n    id: \"{id}\",\n    label: Message::FooBar,\n    places: &[{places}],\n    \
             applies: Applies::Always,\n    icon: {icon},\n}},\n"
        )
    }

    /// One binding: `id` is a Rust expression, so a test may name a literal or a const.
    fn binding(id: &str, key: &str, chord: &str, label: &str) -> String {
        format!("Binding {{ id: {id}, key: egui::Key::{key}, chord: Chord::{chord}, label: \"{label}\" }},\n")
    }

    /// The inventory source a test writes: rows, the shell actions and the key table (U96).
    fn inventory(rows: &[String], keys: &[String], actions: &str) -> String {
        format!(
            "pub const FILE_SAVE: &str = \"file.save\";\n\
             pub const ACTIONS: &[Action] = &[{actions}];\n\
             const ROWS: &[Row] = &[\n{}];\n\
             const KEYS: &[Binding] = &[\n{}];\n",
            rows.concat(),
            keys.concat()
        )
    }

    /// The one shell action every fixture declares, so the action rule has a healthy default.
    const ONE_ACTION: &str = "Action { id: FILE_SAVE, words: Message::ShortcutSave },";
    const ONE_KEY: &str = "Binding { id: FILE_SAVE, key: egui::Key::S, chord: Chord::Ctrl, label: \"Ctrl+S\" },\n";

    fn map(cell: &str) -> String {
        format!("const MAP: [GroupSpec; 1] = [GroupSpec {{ id: \"g\", label: Message::Group, tools: &[{cell}] }}];\n")
    }

    fn spec_tree(name: &str, inventory_source: &str, cell: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("xtask-inventory-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let files = [
            ("crates/engine/src/commands.rs", REGISTRY.to_owned()),
            ("crates/engine/src/foo_cmds.rs", FOO.to_owned()),
            ("crates/ui/src/inventory.rs", inventory_source.to_owned()),
            ("crates/ui/src/toolbars/spec.rs", map(cell)),
        ];
        for (relative, contents) in files {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().expect("a file has a parent")).expect("writable");
            std::fs::write(&path, contents).expect("writable");
        }
        root
    }

    fn problems(root: &Path) -> Vec<String> {
        let mut violations = Vec::new();
        check(root, &mut violations);
        violations
    }

    /// A registered command, a row for it and a drawn cell: the gate is quiet.
    const DRAWN: &str = "ToolSpec { id: \"g.cell\", label: Message::FooBar, icon: Some(icons::contour::LINE), run: Some(ToolKind::FrameAll), method: None, variants: &[] }";

    #[test]
    fn a_healthy_inventory_passes() {
        let rows = [row("foo.bar", "Place::Ribbon", "None")];
        let root = spec_tree("ok", &inventory(&rows, &[ONE_KEY.to_owned()], ONE_ACTION), DRAWN);
        assert_eq!(problems(&root), Vec::<String>::new());
        let mut violations = Vec::new();
        assert_eq!(check(&root, &mut violations), Some(3), "the inventory, the registry and the map");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_command_without_a_row_is_named() {
        let rows = [row("foo.other", "Place::Card", "None")];
        let root = spec_tree("missing", &inventory(&rows, &[ONE_KEY.to_owned()], ONE_ACTION), DRAWN);
        let violations = problems(&root);
        assert!(
            violations.iter().any(|violation| violation.contains("the command `foo.bar`")
                && violation.contains("`foo_cmds::BAR`")
                && violation.contains("has no row")),
            "{violations:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_row_for_an_unknown_id_is_named() {
        let rows = [row("foo.bar", "Place::Ribbon", "None"), row("foo.qux", "Place::Card", "None")];
        let root = spec_tree("unknown", &inventory(&rows, &[ONE_KEY.to_owned()], ONE_ACTION), DRAWN);
        let violations = problems(&root);
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("`foo.qux`") && violation.contains("no registered command")),
            "{violations:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_api_row_that_claims_a_surface_is_named() {
        let rows = [row("foo.bar", "Place::Api, Place::Card", "None")];
        let root = spec_tree("api", &inventory(&rows, &[ONE_KEY.to_owned()], ONE_ACTION), DRAWN);
        let violations = problems(&root);
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("`foo.bar`") && violation.contains("API and on a surface")),
            "{violations:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn two_rows_for_one_id_are_named() {
        let rows = [row("foo.bar", "Place::Ribbon", "None"), row("foo.bar", "Place::Card", "None")];
        let root = spec_tree("duplicate", &inventory(&rows, &[ONE_KEY.to_owned()], ONE_ACTION), DRAWN);
        let violations = problems(&root);
        assert!(violations.iter().any(|violation| violation.contains("second row for `foo.bar`")), "{violations:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn one_key_bound_twice_is_named() {
        let rows = [row("foo.bar", "Place::Ribbon", "None")];
        let keys = [binding("\"foo.bar\"", "G", "Plain", "G"), binding("FILE_SAVE", "G", "Plain", "G")];
        let root = spec_tree("two-keys", &inventory(&rows, &keys, ONE_ACTION), DRAWN);
        let violations = problems(&root);
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("binds `G` to `file.save`")
                    && violation.contains("already carries it")),
            "{violations:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_key_for_an_id_nothing_runs_is_named() {
        let rows = [row("foo.bar", "Place::Ribbon", "None")];
        let keys = [ONE_KEY.to_owned(), binding("\"foo.nowhere\"", "Q", "Plain", "Q")];
        let root = spec_tree("key-unknown", &inventory(&rows, &keys, ONE_ACTION), DRAWN);
        let violations = problems(&root);
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("`foo.nowhere`") && violation.contains("no method of")),
            "{violations:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_key_under_a_method_id_is_accepted() {
        // A drawing key is filed under the map's method id, not a command id (U96).
        let rows = [row("foo.bar", "Place::Ribbon", "None")];
        let keys = [ONE_KEY.to_owned(), binding("\"g.cell\"", "L", "Plain", "L")];
        let root = spec_tree("key-method", &inventory(&rows, &keys, ONE_ACTION), DRAWN);
        assert_eq!(problems(&root), Vec::<String>::new());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_shell_action_without_a_key_is_named() {
        let rows = [row("foo.bar", "Place::Ribbon", "None")];
        let actions = "Action { id: FILE_SAVE, words: Message::ShortcutSave }, Action { id: \"file.new\", words: Message::ShortcutNewScene },";
        let root = spec_tree("action-no-key", &inventory(&rows, &[ONE_KEY.to_owned()], actions), DRAWN);
        let violations = problems(&root);
        assert!(
            violations.iter().any(|violation| violation.contains("`file.new`") && violation.contains("has no key")),
            "{violations:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_label_the_command_does_not_print_is_named() {
        let rows = [row("foo.bar", "Place::Card", "None").replace("Message::FooBar", "Message::SomethingElse")];
        let root = spec_tree("label", &inventory(&rows, &[ONE_KEY.to_owned()], ONE_ACTION), DRAWN);
        let violations = problems(&root);
        assert!(
            violations.iter().any(|violation| violation.contains("`foo.bar`")
                && violation.contains("Message::SomethingElse")
                && violation.contains("Message::FooBar")),
            "{violations:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_live_cell_without_art_is_named() {
        let bare = "ToolSpec { id: \"g.cell\", label: Message::FooBar, icon: None, run: Some(ToolKind::FrameAll), method: None, variants: &[] }";
        let rows = [row("foo.bar", "Place::Ribbon", "None")];
        let root = spec_tree("art", &inventory(&rows, &[ONE_KEY.to_owned()], ONE_ACTION), bare);
        let violations = problems(&root);
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("live cell `g.cell`") && violation.contains("no artwork")),
            "{violations:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_tree_without_an_inventory_is_not_read() {
        let root = std::env::temp_dir().join(format!("xtask-inventory-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("writable");
        let mut violations = Vec::new();
        assert_eq!(check(&root, &mut violations), None);
        assert!(violations.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
