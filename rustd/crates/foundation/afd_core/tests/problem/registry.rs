use std::collections::BTreeSet;

use super::*;

const PUBLISHED_DOCS_BASE: &str = "https://docs.agentsfleet.net/api-reference/error-codes#";

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
    assert_eq!(
        DOCS_BASE, PUBLISHED_DOCS_BASE,
        "the docs base does not match the one the documentation site serves"
    );
}

/// Client-facing problem metadata keeps a floor.
///
/// There is no independent record of every status, title and sentence to pin
/// `entries()` against, and a table copied from `entries()` would only assert
/// that `entries()` equals itself (RULE TCF). What survives an accidental edit
/// is the SHAPE each entry must hold, which is what this asserts — a status a
/// client can act on, prose that is present and is not a placeholder, and a
/// hint an integrator can read.
#[test]
fn test_every_entry_holds_the_shape_a_client_can_act_on() {
    for entry in entries() {
        assert_status_is_one_a_client_handles(entry);
        assert_prose_is_written_for_a_reader(entry);
    }
}

/// The entry's status is in the set `Problem` may answer with, and the three
/// statuses only one code owns are answered by that code alone.
fn assert_status_is_one_a_client_handles(entry: &Problem) {
    let code = entry.code().as_str();

    // A status outside this set reaches a client as an unhandled branch.
    // The set is the one `Problem` is allowed to answer with; a new member
    // is a public-contract decision that belongs in a spec, not a typo.
    assert!(
        matches!(
            entry.status(),
            400 | 401
                | 402
                | 403
                | 404
                | 409
                | 410
                | 412
                | 413
                | 422
                | 424
                | 428
                | 429
                | 500
                | 502
                | 503
                | 504
        ),
        "{code} answers {}, which is not a status this registry may use",
        entry.status()
    );

    if entry.status() == 504 {
        assert_eq!(entry.code(), error_code::AUTHORING_TIMEOUT);
    }
    if entry.status() == 422 {
        assert_eq!(entry.code(), error_code::AUTHORING_TOKEN);
    }
    if entry.status() == 428 {
        assert_eq!(entry.code(), error_code::AUTHORING_PRECONDITION);
    }
}

/// The entry's title, hint and optional dashboard sentence are present and are
/// not placeholders.
fn assert_prose_is_written_for_a_reader(entry: &Problem) {
    let code = entry.code().as_str();

    assert!(!entry.title().is_empty(), "{code} has no title");
    assert!(
        !entry.title().ends_with('.'),
        "{code}: a title is a label, not a sentence — {:?}",
        entry.title()
    );

    // The hint is what an integrator reads to act. An entry that carries
    // none sends them to the code and nothing else.
    assert!(!entry.hint().is_empty(), "{code} has no hint");

    // `eu()` authored a dashboard sentence and `e()` did not. Which one a
    // code used is a fact about who reads it; an EMPTY sentence is neither,
    // and renders as blank space where an instruction belongs.
    if let Some(message) = entry.user_message() {
        assert!(
            !message.is_empty(),
            "{code} carries an empty dashboard sentence — omit it or write it"
        );
        assert!(
            message.ends_with('.') || message.ends_with('!'),
            "{code}: a dashboard sentence is read by a person — {message:?}"
        );
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
    assert_ne!(problem.hint(), "");
}
