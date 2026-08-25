//! Every refusal carries its registry code and the sentence a client reads.
//!
//! The Zig daemon spells a refusal as a pair of arguments at a call site —
//! `ctx.fail(errors.ERR_UNAUTHORIZED, S_INVALID_OR_MISSING_TOKEN)` — twelve
//! times across four middleware files. Nothing relates the two, so nothing can
//! check that the pairing is the same at every site, and the detail strings are
//! per-file constants that happen to agree.
//!
//! This file checks what those twelve sites cannot: that the pairing is a
//! function, that no two refusals collapse onto one another by accident, and —
//! the part that is genuinely parity rather than hygiene — that every sentence
//! a client reads is byte-identical to the Zig constant it replaces.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use afd_auth::error::{Error, Unavailable};

/// The Zig tree whose constants these strings are pinned against.
///
/// Read whole rather than file-by-file: the strings are spread across four
/// middlewares, and which file holds which is not a fact worth encoding here.
const ZIG_AUTH_DIR: &str = "src/agentsfleetd/auth";

fn repo_root() -> PathBuf {
    // <repo>/rustd/crates/afd_auth -> <repo>
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("the crate sits three levels under the repository root")
        .to_path_buf()
}

/// Every `.zig` source under the auth tree, concatenated.
fn zig_auth_sources() -> String {
    let root = repo_root().join(ZIG_AUTH_DIR);
    let mut out = String::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("a readable directory entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "zig") {
                out.push_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| {
                    panic!("cannot read {}: {e}", path.display());
                }));
            }
        }
    }
    assert!(
        !out.is_empty(),
        "found no Zig sources under {}",
        root.display()
    );
    out
}

/// The pairing is a function, and the catalogue is total.
///
/// `ALL` is what every walk below iterates, so a variant missing from it would
/// make this whole file vacuous — the count assertion is what stops that.
#[test]
fn test_every_refusal_has_exactly_one_code_and_one_sentence() {
    assert_eq!(
        Error::ALL.len(),
        7,
        "a new variant must be added to ALL or every walk here silently skips it"
    );

    let mut codes = BTreeSet::new();
    let mut details = BTreeSet::new();
    for err in Error::ALL {
        assert!(
            codes.insert(err.code().as_str()),
            "{err:?} shares a code with an earlier variant"
        );
        assert!(
            details.insert(err.detail()),
            "{err:?} shares its sentence with an earlier variant"
        );
        // `Display` and `detail` must not be two places a sentence is written.
        assert_eq!(err.to_string(), err.detail(), "{err:?}");
        assert!(!err.detail().is_empty(), "{err:?} says nothing");
    }
}

/// The codes are the ones the Zig registry declares, spelled identically.
#[test]
fn test_each_refusal_answers_the_documented_registry_code() {
    for (err, code) in [
        (Error::InvalidOrMissingToken, "UZ-AUTH-002"),
        (Error::TokenExpired, "UZ-AUTH-003"),
        (Error::Unavailable, "UZ-AUTH-004"),
        (Error::TenantKeyRevoked, "UZ-APIKEY-004"),
        (Error::CliCredentialRevoked, "UZ-AUTH-023"),
        (Error::InvalidRunnerToken, "UZ-RUN-001"),
        (Error::RunnerStateBlocked, "UZ-RUN-009"),
    ] {
        assert_eq!(err.code().as_str(), code, "{err:?}");
    }
}

/// Every sentence a client reads appears verbatim in the Zig auth tree.
///
/// This is the assertion that makes the port's behavioural parity checkable
/// rather than asserted. A detail string is as client-visible as a status code:
/// changing one here without changing the Zig daemon would make two binaries
/// answer the same request differently, which is the definition of a behaviour
/// divergence and therefore a bug.
#[test]
fn test_every_client_visible_sentence_is_pinned_to_the_zig_daemons() {
    let zig = zig_auth_sources();
    for err in Error::ALL {
        let quoted = format!("\"{}\"", err.detail());
        assert!(
            zig.contains(&quoted),
            "{err:?} says {quoted}, which appears nowhere under {ZIG_AUTH_DIR}; \
             Zig is the behaviour source of truth"
        );
    }
}

/// The runner plane's sentence is deliberately NOT the tenant plane's.
///
/// `runner_bearer.zig` names the runner token; `bearer_or_api_key.zig` does
/// not. Collapsing them would be a readable simplification and a behaviour
/// divergence, so it is pinned rather than left to review.
#[test]
fn test_the_runner_plane_names_the_runner_token_in_its_refusal() {
    assert_eq!(
        Error::InvalidOrMissingToken.detail(),
        "Invalid or missing token"
    );
    assert_eq!(
        Error::InvalidRunnerToken.detail(),
        "Invalid or missing runner token"
    );
    assert_ne!(
        Error::InvalidOrMissingToken.detail(),
        Error::InvalidRunnerToken.detail()
    );
}

/// Exactly one refusal is not a verdict on the caller.
///
/// The runner client counts consecutive REJECTIONS toward a self-termination
/// ceiling and resets that counter on anything else, so this partition is the
/// difference between a Postgres blip and a fleet walking itself to shutdown.
#[test]
fn test_only_an_outage_is_not_a_rejection() {
    let not_rejections: Vec<_> = Error::ALL
        .into_iter()
        .filter(|err| !err.is_rejection())
        .collect();
    assert_eq!(not_rejections, vec![Error::Unavailable]);
}

/// A dependency's own reason never reaches the decision — every unreachable
/// datastore and every unreachable provider becomes the same refusal.
#[test]
fn test_an_unavailable_dependency_becomes_the_outage_refusal() {
    assert_eq!(Error::from(Unavailable), Error::Unavailable);
    // It renders as what a client would be told, so a log line and a response
    // body cannot disagree about what happened.
    assert_eq!(
        Unavailable.to_string(),
        Error::Unavailable.detail(),
        "the seam error and the refusal must say the same thing"
    );
}

/// A refusal is an `Error`, so it composes with `?` and renders in a chain.
#[test]
fn test_a_refusal_is_an_error() {
    fn as_error(err: Error) -> Box<dyn std::error::Error> {
        Box::new(err)
    }
    for err in Error::ALL {
        assert_eq!(as_error(err).to_string(), err.detail());
    }
}
