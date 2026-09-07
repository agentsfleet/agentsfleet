//! The rate constants this crate declares, pinned against the TypeScript
//! surfaces that echo them.
//!
//! The daemon is the source of truth — the server is what enforces a charge —
//! and two clients spell the same numbers for display: the dashboard reads a
//! tenant balance in nanos, and `agentsfleet doctor --json` prints a billing
//! block. A drift between any two of the three is either a billing-display lie
//! or a server-versus-command-line disagreement, and neither shows up as a
//! failure anywhere else: all three still compile, and both clients still
//! render a plausible number.
//!
//! A shell audit held this line until the rule pack that owned it dropped the
//! script. `audits/ufs.sh` does scan cross-runtime parity, but only over the
//! `ERR_*` prefix, so nothing was left watching the rates. It lives here now
//! rather than in a gate script because the values it compares against are
//! this crate's own constants, and a test beside them cannot go looking at a
//! path that no longer exists.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unreadable mirror file or an unparsable constant \
              is an unmet precondition, and failing loudly on it is the point"
)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use afd_billing::{NANOS_PER_USD, RUN_NANOS_PER_SEC};

/// The TypeScript files that mirror this crate's rate constants.
///
/// Repository-relative, because the pin is about WHICH files agree — a path
/// that stops resolving has to fail the test, not quietly shrink its coverage.
const MIRRORS: [&str; 2] = [
    "ui/packages/app/lib/types.ts",
    "cli/src/constants/billing.ts",
];

/// Every constant both mirrors are expected to declare.
///
/// Two of these carry a pin below; all four are named here because the guard
/// this list serves is about the EXTRACTOR still seeing the file, and a name
/// that quietly vanished from a mirror is the same defect as a parser that
/// stopped reading it.
const MIRRORED_NAMES: [&str; 4] = [
    "NANOS_PER_USD",
    "STARTER_CREDIT_NANOS",
    "EVENT_NANOS",
    "RUN_NANOS_PER_SEC",
];

/// The `export const` prefix a mirrored declaration is spelled with.
const EXPORT_CONST: &str = "export const ";

/// TypeScript's line-comment opener, stripped before an expression is read.
const LINE_COMMENT: &str = "//";

/// `crates/afd_billing` sits three levels below the repository root.
const CRATE_DEPTH_BELOW_ROOT: usize = 3;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(CRATE_DEPTH_BELOW_ROOT)
        .expect("crates/afd_billing is three levels below the repository root")
        .to_path_buf()
}

/// Splits `export const NAME = EXPR;` into its name and its expression text.
///
/// Anything else — an import, a type, a blank line — answers `None`, so the
/// caller walks a file without needing to know its shape.
fn export_const(line: &str) -> Option<(&str, &str)> {
    let (name, expr) = line.trim().strip_prefix(EXPORT_CONST)?.split_once('=')?;
    let expr = expr
        .split(LINE_COMMENT)
        .next()?
        .trim()
        .trim_end_matches(';')
        .trim();
    Some((name.trim(), expr))
}

/// Evaluates the two expression shapes a mirrored rate is written in: an
/// integer literal, and a product of literals and names declared above it.
///
/// `5 * NANOS_PER_USD` is a real spelling in both mirrors, so reading only
/// literals would silently skip the constant most worth pinning. Everything
/// richer than a product — an object, a call, a template string — answers
/// `None` and is simply not a constant this test can compare.
fn eval(expr: &str, seen: &BTreeMap<String, i64>) -> Option<i64> {
    expr.split('*').try_fold(1_i64, |acc, term| {
        let term = term.trim();
        let value = match term.replace('_', "").parse::<i64>() {
            Ok(literal) => literal,
            Err(_) => *seen.get(term)?,
        };
        acc.checked_mul(value)
    })
}

/// Every integer-valued `export const` in a TypeScript source, in file order.
fn exported_ints(source: &str) -> BTreeMap<String, i64> {
    let mut out = BTreeMap::new();
    for line in source.lines() {
        let Some((name, expr)) = export_const(line) else {
            continue;
        };
        if let Some(value) = eval(expr, &out) {
            out.insert(name.to_owned(), value);
        }
    }
    out
}

/// Each mirror's exported integers, read from disk.
fn mirrors() -> Vec<(&'static str, BTreeMap<String, i64>)> {
    let root = repo_root();
    MIRRORS
        .iter()
        .map(|relative| {
            let path = root.join(relative);
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            (*relative, exported_ints(&source))
        })
        .collect()
}

/// Asserts one named constant carries `expected` in every mirror.
fn assert_pinned(name: &str, expected: i64) {
    for (path, constants) in mirrors() {
        let found = constants.get(name).copied().unwrap_or_else(|| {
            panic!(
                "{path} no longer exports {name}; the daemon still charges on it, \
                 so a client that dropped it is displaying something else"
            )
        });
        assert_eq!(
            found, expected,
            "{path} spells {name} as {found}, the daemon enforces {expected}"
        );
    }
}

#[test]
fn test_run_rate_pins_across_every_client_surface() {
    assert_pinned("RUN_NANOS_PER_SEC", RUN_NANOS_PER_SEC);
}

#[test]
fn test_dollar_scale_pins_across_every_client_surface() {
    assert_pinned("NANOS_PER_USD", NANOS_PER_USD);
}

/// The failure this test exists to make impossible: a parser that matches
/// nothing passes every pin above by vacuum.
///
/// A count floor would not do it. Both mirrors export more integers than the
/// pins read, so an extractor that broke down to any two names would clear a
/// floor while dropping the rest from view. Naming the four is what makes the
/// guard fail on a mirror that loses one, or on a parser that stops seeing it.
#[test]
fn test_each_mirror_still_declares_every_pinned_name() {
    for (path, constants) in mirrors() {
        for name in MIRRORED_NAMES {
            assert!(
                constants.contains_key(name),
                "{path} yields no {name}; either the mirror dropped it or the \
                 extractor stopped matching this file's syntax, and the pins \
                 above are vacuous either way"
            );
        }
    }
}

#[test]
fn test_the_extractor_reads_products_and_ignores_richer_expressions() {
    let source = concat!(
        "export const NANOS_PER_USD = 1_000_000_000;\n",
        "export const STARTER_CREDIT_NANOS = 5 * NANOS_PER_USD;\n",
        "export const RUN_NANOS_PER_SEC = 100_000; // trailing prose\n",
        "export const CHARGE_TYPE = Object.freeze({ receive: \"receive\" });\n",
        "const NOT_EXPORTED = 7;\n",
    );
    let constants = exported_ints(source);

    // The fixture above spells these in TypeScript and the assertions spell
    // them as integers. Naming either side would let the extractor and its
    // expectation drift together, which is the one failure this case exists
    // to catch — so both stay literal.
    // pin test: literal is the contract
    assert_eq!(constants.get("NANOS_PER_USD"), Some(&1_000_000_000));
    assert_eq!(constants.get("STARTER_CREDIT_NANOS"), Some(&5_000_000_000));
    // pin test: literal is the contract
    assert_eq!(constants.get("RUN_NANOS_PER_SEC"), Some(&100_000));
    assert_eq!(constants.get("CHARGE_TYPE"), None);
    assert_eq!(constants.get("NOT_EXPORTED"), None);
}

/// A name the mirrors declare but this crate does not is not a drift the pins
/// above can see, so the divergence is stated rather than assumed away.
#[test]
fn test_a_mirror_that_forgets_a_pinned_name_fails_loudly() {
    let constants = exported_ints("export const SOMETHING_ELSE = 1;\n");
    assert_eq!(constants.get("RUN_NANOS_PER_SEC"), None);
}
