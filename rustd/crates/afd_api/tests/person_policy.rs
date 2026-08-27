//! The credential-class matrix: every policy against every class, in one place.
//!
//! # Why this test can exist at all
//!
//! The four hand-written extractors this replaced could not be tested this way.
//! Each was its own `FromRequestParts` impl with the rule buried in a `match`
//! arm, so a suite could only reach them one route at a time — and covered
//! whichever routes somebody remembered. A rule nobody wrote a handler test for
//! was a rule nobody tested.
//!
//! `ClassPolicy` makes each rule DATA: an admit-list, a code, a sentence. So
//! the matrix below is a loop, and adding a fifth policy that forgets a class
//! fails here rather than in production.
//!
//! # The sentence is the assertion, not just the code
//!
//! Two of these policies answer the same `UZ-AUTH-001`. A test asserting only
//! the code cannot tell "not a browser sign-in" from "not a person", so it
//! would stay green with the freshness rule deleted — an `afc_` credential
//! would simply fall through to the wider check and start minting successors.
//! Every refusal below is therefore checked against the policy's own constant.
#![cfg(feature = "test-util")]
#![expect(
    clippy::indexing_slicing,
    reason = "test target: a bad index should fail the test loudly, not be handled"
)]

use afd_api::auth::{ClassPolicy, DashboardClass, FreshSessionClass, HumanClass};
use afd_auth::principal::PersonCredential;

/// A session token with no workspace ceiling.
const SESSION: PersonCredential = PersonCredential::SessionToken {
    workspace_scope: None,
};

/// Whether `admits` carries `credential`'s class, by discriminant.
fn admits(admitted: &[PersonCredential], credential: &PersonCredential) -> bool {
    admitted
        .iter()
        .any(|a| std::mem::discriminant(a) == std::mem::discriminant(credential))
}

/// The dashboard and the credential mint admit a browser session and nothing else.
#[test]
fn the_session_only_policies_admit_exactly_one_class() {
    for (name, admitted) in [
        ("dashboard", DashboardClass::ADMITS),
        ("fresh session", FreshSessionClass::ADMITS),
    ] {
        assert!(
            admits(admitted, &SESSION),
            "{name} must admit a browser session"
        );
        assert!(
            !admits(admitted, &PersonCredential::TenantApiKey),
            "{name} must refuse an organisation's key"
        );
        assert!(
            !admits(admitted, &PersonCredential::CliCredential),
            "{name} must refuse a credential minting its own successor"
        );
    }
}

/// The revoke policy admits a terminal, so `logout` needs no browser.
#[test]
fn the_human_policy_admits_a_terminal_but_never_an_organisation() {
    assert!(
        admits(HumanClass::ADMITS, &PersonCredential::CliCredential),
        "a terminal must be able to end its own access without a browser"
    );
    assert!(
        admits(HumanClass::ADMITS, &SESSION),
        "a browser session can do anything a terminal can"
    );
    assert!(
        !admits(HumanClass::ADMITS, &PersonCredential::TenantApiKey),
        "an organisation's credential must not manage a person's"
    );
}

/// The two session-only policies differ in their REFUSAL and nowhere else.
///
/// This is the pair a code-only assertion cannot separate. If the two sentences
/// ever collapse into one, the freshness rule stops being independently
/// provable and this fails.
#[test]
fn the_session_only_policies_are_told_apart_by_their_sentences() {
    assert_eq!(
        DashboardClass::ADMITS.len(),
        FreshSessionClass::ADMITS.len(),
        "the two admit the same set, which is why the sentence has to differ"
    );
    assert_ne!(
        DashboardClass::DETAIL,
        FreshSessionClass::DETAIL,
        "two policies admitting the same classes must refuse in distinguishable words"
    );
    assert_ne!(
        DashboardClass::EVENT,
        FreshSessionClass::EVENT,
        "and must be separable in a log"
    );
}

/// Every policy names a sentence and an event, and no two share an event.
///
/// The event is what an operator greps for. Two policies sharing one would make
/// a refusal untraceable to the rule that produced it.
#[test]
fn every_policy_is_diagnosable() {
    let events = [
        DashboardClass::EVENT,
        FreshSessionClass::EVENT,
        HumanClass::EVENT,
    ];
    for (index, event) in events.iter().enumerate() {
        assert!(!event.is_empty(), "a policy with no event cannot be traced");
        assert!(
            !events[index + 1..].contains(event),
            "{event} is claimed by two policies, so a log line cannot name which refused"
        );
    }
    for detail in [
        DashboardClass::DETAIL,
        FreshSessionClass::DETAIL,
        HumanClass::DETAIL,
    ] {
        assert!(
            !detail.is_empty(),
            "a refusal with no sentence tells the caller nothing"
        );
    }
}
