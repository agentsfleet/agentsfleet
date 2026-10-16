//! Shared scaffolding for this crate's native Definition corpus.
//!
//! Every corpus suite reads the SAME files from the same place on disk, so the
//! corpus stays one oracle rather than several that can drift. What lives here
//! is only the reading — each suite owns its own assertions.

#![allow(
    dead_code,
    reason = "each test binary compiles its own copy and uses a different subset of it, so an item unused by one is not dead code in any real sense"
)]

use std::path::PathBuf;

use afd_fleet_runtime::Definition;

/// The corpus root, relative to this crate.
///
/// The fixtures live at the repository root and cargo runs a test from the
/// package directory, so the walk up is spelled out rather than assumed.
const CORPUS: &str = "../../../tests/fixtures/fleets";

/// The file each first-party fleet directory holds its native setup in.
const DEFINITION_FILE: &str = "definition.json";

/// The placeholder the `platform-ops` and `steer-probe` setups carry in place
/// of a runtime model, quoted as the JSON string it stands in.
const MODEL_PLACEHOLDER: &str = "{{model}}";

/// The value [`fixture`] substitutes for [`MODEL_PLACEHOLDER`].
pub(crate) const MODEL_VALUE: &str = "accounts/fireworks/models/kimi-k2.6";

/// The placeholder standing in for a context ceiling, including its quotes:
/// the filled value is a JSON number, so the string around it goes too.
const CONTEXT_CAP_PLACEHOLDER: &str = "\"{{context_cap_tokens}}\"";

/// The value [`fixture`] substitutes for [`CONTEXT_CAP_PLACEHOLDER`].
pub(crate) const CONTEXT_CAP_VALUE: u32 = 256_000;

/// The one tool every first-party fleet reaches its service through.
pub(crate) const TOOL_HTTP_REQUEST: &str = "http_request";

/// One slug per first-party fleet, and the slug IS the identity: it names the
/// fixture directory and the `config.name` its Definition declares.
pub(crate) const FIRST_PARTY: [&str; 9] = [
    "github-pr-reviewer",
    "security-reviewer",
    "zoho-sprint-daily-summarizer",
    "zoho-recruiter-daily-summarizer",
    "incident-responder",
    "incident-repairer",
    "incident-verifier",
    "ci-responder",
    "ci-repairer",
];

/// One fixture's bytes, exactly as they sit on disk.
pub(crate) fn raw_fixture(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(CORPUS)
        .join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|_missing| panic!("corpus fixture {} should be readable", path.display()))
}

/// One fixture's bytes, with the corpus's placeholder substitutions applied.
///
/// Templated setups are not standalone-valid: a quoted placeholder sits where
/// the context ceiling's number belongs. A harness that forgot the substitution
/// would report a corpus regression that is really its own bug.
pub(crate) fn fixture(relative: &str) -> String {
    raw_fixture(relative)
        .replace(MODEL_PLACEHOLDER, MODEL_VALUE)
        .replace(CONTEXT_CAP_PLACEHOLDER, &CONTEXT_CAP_VALUE.to_string())
}

/// The relative path of a first-party fleet's native setup.
pub(crate) fn definition_path(slug: &str) -> String {
    format!("{slug}/{DEFINITION_FILE}")
}

/// A corpus Definition that must parse, with its placeholders filled.
pub(crate) fn definition(relative: &str) -> Definition {
    Definition::parse(&fixture(relative))
        .unwrap_or_else(|failure| panic!("{relative} should parse: {failure}"))
}
