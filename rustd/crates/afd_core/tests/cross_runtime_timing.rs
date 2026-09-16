//! The lease clock, pinned across the two runtimes that keep it.
//!
//! `LEASE_TTL_MS` and its four relatives are declared TWICE: here in
//! [`afd_core::timing`], which the daemon issues and stamps leases from, and in
//! `src/lib/common/constants.zig`, which the Zig runner renews and heartbeats
//! from. Until this file existed the only thing holding them equal was a doc
//! comment on each side saying the other one said the same.
//!
//! That is not enough, and the failure is silent in both directions. Raise the
//! daemon's TTL and the runner keeps renewing on the old deadline. Raise the
//! runner's and it heartbeats against a window the daemon will not honour. No
//! compiler sees either, because neither language can read the other's source.
//!
//! A third reader makes it worse: the Grafana alerting playbook derives the
//! runner-offline threshold by running `sed` over the ZIG file
//! (`playbooks/operations/observability/providers/grafana/common.sh`), so an
//! alert threshold is computed from the runner's copy while the behaviour it
//! alerts on is the daemon's. With this test the copies cannot disagree, so it
//! no longer matters which one anything reads.
//!
//! The shape is `afd_billing`'s `cross_runtime_rates.rs`, which pins the money
//! constants across the app and the Command-Line Interface (CLI) mirrors for
//! the same reason. This is that test with a Zig parser.

use std::collections::BTreeMap;
use std::path::PathBuf;

use afd_core::timing;

/// The Zig mirror of this crate's timing constants.
const ZIG_MIRROR: &str = "src/lib/common/constants.zig";

/// What a public integer constant opens with in Zig.
const PUB_CONST: &str = "pub const ";

/// Zig's line-comment opener, stripped before an expression is read.
const LINE_COMMENT: &str = "//";

/// The type annotation the mirrored constants carry.
const TYPE_SUFFIX: &str = ": i64";

/// `crates/afd_core` sits three levels below the repository root.
const CRATE_DEPTH_BELOW_ROOT: usize = 3;

fn repo_root() -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for _ in 0..CRATE_DEPTH_BELOW_ROOT {
        root.pop();
    }
    root
}

/// Splits `pub const NAME: i64 = EXPR;` into its name and its expression text.
///
/// Anything else — a private constant, a function, a comment — answers `None`,
/// so the caller walks the file without knowing its shape. Private is the right
/// thing to skip: a constant the runner does not export is not a shared clock,
/// and demanding one here would make this test a visibility rule.
fn pub_const(line: &str) -> Option<(&str, &str)> {
    let (name, expr) = line.trim().strip_prefix(PUB_CONST)?.split_once('=')?;
    let name = name.trim().strip_suffix(TYPE_SUFFIX)?.trim();
    let expr = expr
        .split(LINE_COMMENT)
        .next()?
        .trim()
        .trim_end_matches(';')
        .trim();
    Some((name, expr))
}

/// Evaluates the two expression shapes these constants are written in: an
/// integer literal, and a product of literals and names declared above it.
///
/// `LEASE_TTL_MS * 3` is a real spelling on both sides, so reading only
/// literals would skip `RUNNER_OFFLINE_AFTER_MS` — the one the alerting
/// playbook actually consumes.
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

/// Every integer-valued `pub const` in the Zig mirror, in file order.
fn mirrored_ints() -> Result<BTreeMap<String, i64>, String> {
    let path = repo_root().join(ZIG_MIRROR);
    let source = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut out = BTreeMap::new();
    for line in source.lines() {
        let Some((name, expr)) = pub_const(line) else {
            continue;
        };
        if let Some(value) = eval(expr, &out) {
            out.insert(name.to_owned(), value);
        }
    }
    Ok(out)
}

/// Asserts one named constant carries `expected` in the Zig mirror.
fn assert_pinned(mirror: &BTreeMap<String, i64>, name: &str, expected: i64) {
    let found = mirror.get(name).copied();
    assert!(
        found.is_some(),
        "{ZIG_MIRROR} no longer exports {name}; the runner keeps the lease \
         clock from it and the Grafana alerting playbook reads it with sed, \
         so dropping the `pub` breaks both without failing a compile"
    );
    assert_eq!(
        found,
        Some(expected),
        "{ZIG_MIRROR} spells {name} as {found:?}, the daemon enforces {expected}"
    );
}

/// The daemon's lease clock and the runner's are the same clock.
#[test]
fn test_the_lease_clock_agrees_across_both_runtimes() -> Result<(), String> {
    let mirror = mirrored_ints()?;
    assert_pinned(&mirror, "LEASE_TTL_MS", timing::LEASE_TTL_MS);
    assert_pinned(&mirror, "RENEWAL_WINDOW_MS", timing::RENEWAL_WINDOW_MS);
    assert_pinned(&mirror, "RENEWAL_TICK_MS", timing::RENEWAL_TICK_MS);
    assert_pinned(
        &mirror,
        "RUNNER_OFFLINE_AFTER_MS",
        timing::RUNNER_OFFLINE_AFTER_MS,
    );
    assert_pinned(
        &mirror,
        "HEARTBEAT_INTERVAL_MS",
        timing::HEARTBEAT_INTERVAL_MS,
    );
    Ok(())
}

/// The parser reads the shapes the mirror is actually written in.
///
/// Without this, a parser that silently matched nothing would make every
/// assertion above pass vacuously on an empty map — the failure mode that makes
/// a pinning test worse than none, because it reports agreement it never
/// checked.
#[test]
fn test_the_mirror_parses_and_is_not_silently_empty() -> Result<(), String> {
    let mirror = mirrored_ints()?;
    assert!(
        mirror.len() >= 5,
        "parsed only {} constants from {ZIG_MIRROR}; the pinning test above \
         would pass vacuously",
        mirror.len()
    );
    assert_eq!(
        pub_const("pub const LEASE_TTL_MS: i64 = 30_000;"),
        Some(("LEASE_TTL_MS", "30_000")),
        "a literal declaration must parse"
    );
    assert_eq!(
        pub_const("pub const RUNNER_OFFLINE_AFTER_MS: i64 = LEASE_TTL_MS * 3;"),
        Some(("RUNNER_OFFLINE_AFTER_MS", "LEASE_TTL_MS * 3")),
        "a product declaration must parse"
    );
    assert_eq!(
        pub_const("const LEASE_TTL_MS: i64 = 30_000;"),
        None,
        "a private constant is not a shared clock"
    );
    Ok(())
}
