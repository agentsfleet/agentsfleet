//! Every code a client can receive carries the status and prose it answers with.
//!
//! The Zig entries are the source of truth, as the codes themselves are. What
//! this file proves is that the table here is TOTAL over the registry and
//! byte-identical to that source — so §5's `application/problem+json` envelope
//! can be assembled from it without a second lookup that could disagree.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use afd_core::error_code::{self, REGISTRY};
use afd_core::problem::{DOCS_BASE, Problem, entries};

/// The Zig files that pair every code with its status and prose.
///
/// TWO of them: `error_entries.zig` carries the control plane and
/// `error_entries_runtime.zig` the execute path, split for the 350-line file
/// cap and concatenated by `error_registry.zig`. Reading only the first is how
/// this test initially "proved" a code had no entry when it had one.
const ZIG_ENTRIES: [&str; 2] = [
    "src/agentsfleetd/errors/error_entries.zig",
    "src/agentsfleetd/errors/error_entries_runtime.zig",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("the crate sits three levels under the repository root")
        .to_path_buf()
}

fn zig_entries() -> String {
    let root = repo_root();
    let mut all = String::new();
    for relative in ZIG_ENTRIES {
        let path = root.join(relative);
        all.push_str(
            &std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display())),
        );
        all.push('\n');
    }
    all
}

/// The Zig entries with every `const NAME = "value";` substituted in.
///
/// Several declarations name a constant rather than a literal — the code
/// itself in `e(S_UZ_INTERNAL_003, …)`, and titles reused across entries as
/// `S_TITLE_REQUEST_FAILED`. Matching the raw text would report those codes as
/// undeclared, which is exactly the false negative that let two fabricated
/// entries through here before this substitution existed. Resolving the
/// constants is the general fix; special-casing the two would leave the next
/// one to be discovered the same way.
fn zig_entries_expanded() -> String {
    let raw = zig_entries();
    let mut constants: Vec<(String, String)> = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim_start().trim_start_matches("pub ");
        let Some(rest) = trimmed.strip_prefix("const ") else {
            continue;
        };
        let Some((name, value)) = rest.split_once(" = ") else {
            continue;
        };
        let value = value.trim_end().trim_end_matches(';');
        if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
            constants.push((name.trim().to_owned(), value.to_owned()));
        }
    }
    // Longest name first, so a name that is a prefix of another cannot shadow it.
    constants.sort_by_key(|(name, _value)| std::cmp::Reverse(name.len()));

    raw.lines()
        .map(|line| {
            let mut expanded = line.to_owned();
            for (name, value) in &constants {
                if expanded.contains(name.as_str()) {
                    expanded = expanded.replace(name.as_str(), value);
                }
            }
            expanded
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The table covers the registry exactly — no code without an entry, and no
/// entry for a code that is not declared.
///
/// The first half is what keeps [`Problem::UNKNOWN`] unreachable: a declared
/// code with no entry would answer 500 "Unknown error" to a client, which is
/// the failure this test exists to make impossible. The second half stops a
/// stale entry outliving the code it described.
#[test]
fn test_every_declared_code_has_an_entry_and_no_entry_is_orphaned() {
    for code in REGISTRY {
        let problem = Problem::of(*code);
        assert_eq!(
            problem.code(),
            *code,
            "{} has no entry, so it would answer as an unknown error",
            code.as_str()
        );
    }

    let declared: BTreeSet<_> = REGISTRY.iter().map(|code| code.as_str()).collect();
    for entry in entries() {
        assert!(
            declared.contains(entry.code().as_str()),
            "{} has an entry but is not declared in REGISTRY",
            entry.code().as_str()
        );
    }
    assert_eq!(entries().len(), REGISTRY.len());
}

/// Each entry's status, title and prose appear verbatim in the Zig entries.
///
/// A status is a property of the CODE, and this is what holds the two binaries
/// to the same one: `UZ-AUTH-022` answering 403 in one and 401 in the other
/// would send a client round a re-authentication loop that never terminates.
#[test]
fn test_entries_match_the_zig_registry() {
    let zig = zig_entries_expanded();
    for entry in entries() {
        let code = entry.code().as_str();
        let declaration = zig
            .lines()
            .find(|line| {
                line.contains(&format!("\"{code}\"")) && line.trim_start().starts_with(['e', 'u'])
            })
            .unwrap_or_else(|| panic!("{code} is not declared in either entries file"));

        assert!(
            declaration.contains(&format!("\"{}\"", entry.title())),
            "{code}: title does not match the Zig entries"
        );
        assert!(
            declaration.contains(zig_status(entry.status())),
            "{code}: status {} does not match the Zig entries",
            entry.status()
        );
        // `eu()` authors a dashboard sentence; `e()` does not. Which one a code
        // uses is itself a fact worth pinning: a sentence appearing on a
        // runner-plane code would be prose nobody reads, and one disappearing
        // from a dashboard code shows a person a hint written for an integrator.
        let authored = declaration.trim_start().starts_with("eu(");
        assert_eq!(
            entry.user_message().is_some(),
            authored,
            "{code}: dashboard sentence present here = {}, in Zig = {authored}",
            entry.user_message().is_some()
        );
        if let Some(message) = entry.user_message() {
            assert!(
                declaration.contains(&format!("\"{message}\"")),
                "{code}: dashboard sentence does not match the Zig entries"
            );
        }
    }
}

/// The Zig spelling of an HTTP status, as `std.http.Status` names it.
fn zig_status(status: u16) -> &'static str {
    match status {
        400 => ".bad_request",
        401 => ".unauthorized",
        403 => ".forbidden",
        500 => ".internal_server_error",
        503 => ".service_unavailable",
        other => panic!("no Zig spelling recorded for status {other}"),
    }
}

/// The documentation link is derived from the code, so it cannot point at
/// another code's anchor.
#[test]
fn test_the_docs_link_is_derived_from_the_code() {
    for entry in entries() {
        let uri = entry.docs_uri();
        assert!(uri.starts_with(DOCS_BASE), "{uri}");
        assert!(uri.ends_with(entry.code().as_str()), "{uri}");
    }
    assert_eq!(
        Problem::of(error_code::AUTH_INSUFFICIENT_SCOPE).docs_uri(),
        format!("{DOCS_BASE}UZ-AUTH-022")
    );
    // And the base is the one the documentation site actually serves.
    let zig = zig_entries();
    assert!(
        zig.contains(&format!("ERROR_DOCS_BASE = \"{DOCS_BASE}\"")),
        "the docs base does not match the Zig entries"
    );
}

/// The statuses the auth plane depends on, stated rather than inferred.
///
/// These four are load-bearing beyond the envelope: `docs/AUTH.md` rests on
/// 022 being a 403 (re-authenticating cannot help), and the runner client
/// classifies 004 as transport loss rather than an auth rejection — which is
/// what stops a datastore outage walking a healthy fleet to shutdown.
#[test]
fn test_the_auth_planes_statuses_are_the_documented_ones() {
    for (code, status) in [
        (error_code::AUTH_INSUFFICIENT_SCOPE, 403),
        (error_code::AUTH_UNAUTHORIZED, 401),
        (error_code::AUTH_TOKEN_EXPIRED, 401),
        (error_code::AUTH_UNAVAILABLE, 503),
        (error_code::AUTH_CLI_CREDENTIAL_REVOKED, 401),
        (error_code::APIKEY_REVOKED, 401),
        (error_code::RUN_INVALID_RUNNER_TOKEN, 401),
        (error_code::RUN_ADMIN_STATE_BLOCKED, 401),
    ] {
        assert_eq!(Problem::of(code).status(), status, "{}", code.as_str());
    }
}

/// An unregistered code degrades to an honest 500 rather than failing.
///
/// Unreachable for a code this workspace declares — the totality test above is
/// what makes that true — but a response is being written when this is reached,
/// and there is nothing better to do than answer.
#[test]
fn test_an_unregistered_code_degrades_to_the_unknown_entry() {
    let stranger = afd_core::error_code::ErrorCode::declare("UZ-NOSUCH-001");
    let problem = Problem::of(stranger);

    assert_eq!(problem, Problem::UNKNOWN);
    assert_eq!(problem.status(), 500);
    assert_eq!(problem.title(), "Unknown error");
    assert!(problem.user_message().is_none());
    assert!(!problem.hint().is_empty());
}
