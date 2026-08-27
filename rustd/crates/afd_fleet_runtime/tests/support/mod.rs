//! Shared scaffolding for this crate's corpus targets.
//!
//! Both corpus suites read the SAME files the Zig suite reads, from the same
//! place on disk, so the corpus stays one oracle rather than two that can
//! drift. What lives here is only the reading — each suite owns its own
//! assertions.

#![allow(
    dead_code,
    reason = "each test binary compiles its own copy and uses a different subset of it, so an item unused by one is not dead code in any real sense"
)]

use std::path::PathBuf;

/// The corpus root, relative to this crate.
///
/// The Zig suite resolves it from the repository root because `zig build` sets
/// the working directory there; cargo sets it to the package, so the walk up is
/// spelled out rather than assumed.
const CORPUS: &str = "../../../tests/fixtures/fleetbundle";

/// The placeholder the `platform-ops` and `steer-probe` documents carry in
/// place of a model.
const MODEL_PLACEHOLDER: &str = "{{model}}";

/// The value the Zig suite substitutes for [`MODEL_PLACEHOLDER`].
pub(crate) const MODEL_VALUE: &str = "accounts/fireworks/models/kimi-k2.6";

/// The placeholder standing in for a context ceiling.
const CONTEXT_CAP_PLACEHOLDER: &str = "{{context_cap_tokens}}";

/// The value the Zig suite substitutes for [`CONTEXT_CAP_PLACEHOLDER`].
const CONTEXT_CAP_VALUE: &str = "256000";

/// The one tool every first-party bundle declares.
pub(crate) const TOOL_HTTP_REQUEST: &str = "http_request";

/// One slug per first-party bundle, and the slug IS the identity: it names the
/// fixture directory, the `name:` both documents declare, and the fleet-library
/// id the importer takes from that frontmatter. A bundle whose declared name
/// drifts from its directory onboards as a second catalogue entry instead of
/// filling the seeded one.
pub(crate) const FIRST_PARTY: [&str; 4] = [
    "github-pr-reviewer",
    "security-reviewer",
    "zoho-sprint-daily-summarizer",
    "zoho-recruiter-daily-summarizer",
];

/// One fixture's bytes, exactly as they sit on disk.
pub(crate) fn raw_fixture(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(CORPUS)
        .join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|_missing| panic!("corpus fixture {} should be readable", path.display()))
}

/// One fixture's bytes, with the Zig suite's placeholder substitutions applied.
///
/// Three documents carry `{{…}}` templates and are not standalone-parseable —
/// `context_cap_tokens: {{context_cap_tokens}}` is UNQUOTED, so raw it is a
/// flow-mapping token and a genuine parse error. A harness that forgot the
/// substitution would report a corpus regression that is really its own bug.
pub(crate) fn fixture(relative: &str) -> String {
    raw_fixture(relative)
        .replace(MODEL_PLACEHOLDER, MODEL_VALUE)
        .replace(CONTEXT_CAP_PLACEHOLDER, CONTEXT_CAP_VALUE)
}
