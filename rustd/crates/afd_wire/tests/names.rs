//! No two schema-bearing types in this crate publish under one name.
//!
//! # Why a source scan and not the generator
//!
//! utoipa keys a document's components by name alone, and a second type
//! registered under a name already taken replaces the first without a word:
//! the lease's egress rules were published as the runner's three-word posture
//! that way, and every reference still resolved. The generator cannot report
//! what it silently merged, so the claim is made where the names are declared:
//! every `ToSchema` derive in `src/` is read with the name it publishes under,
//! an explicit `schema(as = …)` alias or the type's own, and the set must have
//! no duplicates.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unreadable source file is a precondition failure, not a result"
)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The attribute that turns a derive into a published component.
const DERIVE: &str = "derive(utoipa::ToSchema)";

/// The attribute that renames what the derive publishes.
const ALIAS: &str = "schema(as = ";

/// A schema written by hand rather than derived; it publishes the type's name.
const MANUAL: &str = "impl utoipa::ToSchema for ";

fn sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("src is readable") {
            let path = entry.expect("a directory entry is readable").path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut out);
    out.sort();
    out
}

/// The name a derive publishes: its alias, else the type's own name.
fn published_name(lines: &[&str], derive_at: usize) -> Option<String> {
    let mut alias = None;
    // An attribute may span lines (`#[expect(\n    lint,\n    reason = "…"\n)]`);
    // everything until its closing bracket is still the attribute.
    let mut open_attribute = false;
    for line in lines.iter().skip(derive_at + 1) {
        let line = line.trim();
        if let Some(rest) = line.split_once(ALIAS).map(|(_, rest)| rest) {
            let name = rest.trim_end_matches(&[')', ']'][..]);
            alias = Some(name.rsplit("::").next().unwrap_or(name).to_owned());
        }
        if open_attribute {
            open_attribute = !line.ends_with(']');
            continue;
        }
        if line.starts_with('#') {
            open_attribute = !line.ends_with(']');
            continue;
        }
        if line.starts_with("//") {
            continue;
        }
        let item = line
            .trim_start_matches("pub(crate) ")
            .trim_start_matches("pub ");
        let name = item
            .strip_prefix("struct ")
            .or_else(|| item.strip_prefix("enum "))?;
        let own = name
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .next()?
            .to_owned();
        return Some(alias.unwrap_or(own));
    }
    None
}

/// Every published component name is declared exactly once.
#[test]
fn no_two_schema_types_publish_under_one_name() {
    let mut owners: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in sources() {
        let text = fs::read_to_string(&path).expect("a source file is readable");
        let lines: Vec<&str> = text.lines().collect();
        for (at, line) in lines.iter().enumerate() {
            let name = if let Some(rest) = line.trim().strip_prefix(MANUAL) {
                rest.split(['<', ' ', '{'])
                    .next()
                    .unwrap_or(rest)
                    .to_owned()
            } else if line.contains(DERIVE) {
                published_name(&lines, at).unwrap_or_else(|| {
                    panic!("{}:{}: a derive with no item", path.display(), at + 1)
                })
            } else {
                continue;
            };
            owners.entry(name).or_default().push(format!(
                "{}:{}",
                path.strip_prefix(env!("CARGO_MANIFEST_DIR"))
                    .unwrap_or(&path)
                    .display(),
                at + 1
            ));
        }
    }

    assert!(
        !owners.is_empty(),
        "no derives were read at all; this gate would pass against an empty crate"
    );
    let shared: Vec<String> = owners
        .iter()
        .filter(|(_, at)| at.len() > 1)
        .map(|(name, at)| format!("{name}: {}", at.join(", ")))
        .collect();
    assert!(
        shared.is_empty(),
        "two types publish under one component name, and the generator keeps \
         whichever registered second:\n  {}",
        shared.join("\n  ")
    );
}
