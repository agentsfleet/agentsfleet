//! The shipped reviewer bundle addresses the Pull Request the event names.
//!
//! The bundle's `SKILL.md` is prose a model executes, and the `{field}` names
//! it templates into its API paths are read off the event body. That body is
//! the flat [`PullRequestDigest`], never GitHub's raw payload, so every name
//! the bundle templates must be a key the digest serializes. A digest field
//! renamed without the bundle, or a bundle naming a raw-payload path, fails
//! here.
#![expect(
    clippy::expect_used,
    reason = "test target: a serialization failure should fail the test loudly"
)]

use std::borrow::Cow;
use std::collections::BTreeSet;

use afd_api_wire::ingress::PullRequestDigest;

/// The first-party reviewer bundle, as it ships.
const REVIEWER_SKILL: &str =
    include_str!("../../../../tests/fixtures/fleetbundle/github-pr-reviewer/SKILL.md");

/// The path segment after which an API path names one Pull Request.
const PULLS_SEGMENT: &str = "/pulls/";

/// Opens and closes one templated field name.
const OPEN: char = '{';
const CLOSE: char = '}';

/// One delivery, shaped as the daemon digests it.
fn digest() -> PullRequestDigest<'static> {
    PullRequestDigest {
        action: Cow::Borrowed("opened"),
        repo: Cow::Borrowed("agentsfleet/linkwarden"),
        number: 7,
        title: Cow::Borrowed("Tighten the id check"),
        url: Cow::Borrowed("https://github.com/agentsfleet/linkwarden/pull/7"),
        state: Cow::Borrowed("open"),
        draft: false,
        author: Cow::Borrowed("octocat"),
        head_ref: Cow::Borrowed("fix/id-check"),
        base_ref: Cow::Borrowed("main"),
        head_sha: Cow::Borrowed("0123456789abcdef0123456789abcdef01234567"),
        received_at: Cow::Borrowed("2026-10-10T00:00:00Z"),
    }
}

/// The keys a fleet reads off the event body.
fn digest_keys() -> BTreeSet<String> {
    let value = serde_json::to_value(digest()).expect("the digest serializes");
    value
        .as_object()
        .expect("the digest serializes as an object")
        .keys()
        .cloned()
        .collect()
}

/// Every `{name}` the text templates, where `name` is a plain field name.
fn templated_fields(text: &str) -> BTreeSet<&str> {
    text.split(OPEN)
        .skip(1)
        .filter_map(|rest| rest.split_once(CLOSE).map(|(name, _)| name))
        .filter(|name| {
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        })
        .collect()
}

#[test]
fn the_reviewer_bundle_templates_only_fields_the_digest_carries() {
    let keys = digest_keys();
    let templated = templated_fields(REVIEWER_SKILL);

    assert!(
        !templated.is_empty(),
        "the bundle templates no event field at all"
    );
    let unknown: Vec<&str> = templated
        .iter()
        .copied()
        .filter(|name| !keys.contains(*name))
        .collect();
    assert!(
        unknown.is_empty(),
        "the bundle templates {unknown:?}, which the digest does not carry; it carries {keys:?}"
    );
}

#[test]
fn the_reviewer_bundle_reads_its_pull_request_number_from_the_event() {
    let addressed: Vec<&str> = REVIEWER_SKILL.split(PULLS_SEGMENT).skip(1).collect();

    assert!(
        !addressed.is_empty(),
        "the bundle names no Pull Request API path"
    );
    for rest in addressed {
        assert!(
            rest.starts_with(OPEN),
            "a Pull Request path is hard-coded rather than read from the event: {PULLS_SEGMENT}{}",
            rest.lines().next().unwrap_or_default()
        );
    }
}
