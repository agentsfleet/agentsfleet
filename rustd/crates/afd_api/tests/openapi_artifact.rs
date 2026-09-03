//! The committed document is the build's output, and the build ships without it.
//!
//! # Two claims, one file
//!
//! `public/openapi.json` is what an API consumer reads, and it is worth reading
//! only if it is what this daemon actually generates. That is the first test.
//! The second is the other side of the same decision: generation is a build-time
//! tool, so the binary an operator runs must carry none of it.
#![cfg(all(feature = "test-util", feature = "openapi"))]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unreadable artifact is a precondition failure, not a result"
)]

use std::fs;
use std::path::{Path, PathBuf};

/// The published artifact, relative to the repository root.
const ARTIFACT: &str = "public/openapi.json";

/// The feature that turns generation on.
const FEATURE: &str = "openapi";

/// The dependency it is the only way to acquire.
const GENERATOR: &str = "utoipa";

/// The profile the shipped binary is built with.
const DIST_PROFILE: &str = "--profile dist";

/// Every file that builds a shipping binary, relative to the repository root.
const RELEASE_INVOCATIONS: [&str; 5] = [
    "make/build.mk",
    ".github/workflows/release.yml",
    ".github/workflows/deploy-dev-build.yml",
    ".github/workflows/test.yml",
    "Dockerfile",
];

/// The two spellings that would put the generator into a shipped build.
const FEATURE_FLAGS: [&str; 2] = ["--all-features", "--features openapi"];

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("afd_api must remain under <repo>/rustd/crates")
        .to_owned()
}

/// The committed artifact equals what this build emits.
///
/// Byte-for-byte after the trailing newline a file gets and a stream does not.
/// The remedy for a failure is ALWAYS to regenerate — never to hand-patch the
/// file, which is the habit this test exists to end:
///
/// ```text
/// cd rustd && cargo run -q -p agentsfleetd --features openapi \
///   --bin agentsfleetd -- --no-banner openapi > ../public/openapi.json
/// ```
#[test]
fn test_openapi_build_is_the_source() {
    let path = repository_root().join(ARTIFACT);
    let committed = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let emitted = afd_api::openapi::document()
        .to_pretty_json()
        .expect("the generated document serializes");

    assert_eq!(
        committed.trim_end(),
        emitted.trim_end(),
        "{ARTIFACT} is not what this build emits. Regenerate it — never edit it \
         by hand:\n  cd rustd && cargo run -q -p agentsfleetd --features {FEATURE} \
         --bin agentsfleetd -- --no-banner {FEATURE} > ../{ARTIFACT}"
    );
}

/// No shipping build names the feature that would compile the generator in.
///
/// # Why this reads build files rather than measuring a graph
///
/// The graph is measured on the command line (`cargo tree -p afd_wire`), and a
/// test that shelled out to cargo would need a resolver run and a network path
/// inside a suite that must stay hermetic. What can go wrong HERE is that
/// somebody adds `--all-features` to a release invocation for an unrelated
/// reason and silently ships a schema compiler. That is a text fact about the
/// build files, and this is where it is graded.
#[test]
fn test_release_build_excludes_openapi() {
    let root = repository_root();
    let mut offenders = Vec::new();

    for relative in RELEASE_INVOCATIONS {
        let path = root.join(relative);
        let Ok(text) = fs::read_to_string(&path) else {
            panic!("{relative} is named as a release invocation and does not exist");
        };
        for (number, line) in text.lines().enumerate() {
            if !line.contains(DIST_PROFILE) {
                continue;
            }
            for flag in FEATURE_FLAGS {
                if line.contains(flag) {
                    offenders.push(format!("{relative}:{}: {}", number + 1, line.trim()));
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "a shipping build names a feature flag that would compile {GENERATOR} \
         into the daemon:\n  {}",
        offenders.join("\n  ")
    );
}

/// The release invocations this test grades still exist to be graded.
///
/// A file renamed out from under `RELEASE_INVOCATIONS` would make the test
/// above pass by checking nothing. The `panic!` in its `else` arm covers a
/// missing file; this covers the subtler case of a file that exists and no
/// longer builds anything.
#[test]
fn test_the_release_invocations_are_still_release_invocations() {
    let root = repository_root();
    let silent: Vec<&str> = RELEASE_INVOCATIONS
        .into_iter()
        .filter(|relative| {
            fs::read_to_string(root.join(relative)).is_ok_and(|text| !text.contains(DIST_PROFILE))
        })
        .collect();

    assert!(
        silent.is_empty(),
        "these files no longer build a shipping binary, so grading them proves \
         nothing — repoint the roster: {silent:?}"
    );
}
