//! Which key family an index reads, and what a connection says about itself.
//!
//! No server: every claim here is about how a handle is ADDRESSED, not about
//! anything it would find. The live-datastore suites all build their index the
//! same way — `ReadyIndex::under` with a private prefix — so the accessor that
//! reports which family they landed in, and the default the production path
//! relies on, are never read by the tests that use them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_dragonfly::ready::{READY_INDEX_KEY, ReadyIndex, ReadyPrefix};

/// Long enough to be visibly a real setting in the rendering below, short
/// enough that nothing here could wait it out.
const REQUEST_BUDGET: Duration = Duration::from_millis(250);

/// A handle that will never reach a server. Every claim in this file is about
/// how it is addressed, so it never has to. Built inside a runtime because the
/// driver registers its background dialler on construction.
fn pending_connection() -> Dragonfly {
    let config =
        DragonflyConfig::from_url(DragonflyRole::Default, "redis://127.0.0.1:1/".to_owned())
            .with_request_timeout(REQUEST_BUDGET);
    Dragonfly::unreachable(&config).expect("a well-formed URL builds a pending connection")
}

/// The default IS the production family, spelled once. A separate constant
/// here would be a second answer to a question the index already answers, and
/// the two would drift the moment either moved.
#[test]
fn the_default_prefix_is_the_production_family() {
    assert_eq!(ReadyPrefix::default(), ReadyPrefix::production());
    assert_eq!(ReadyPrefix::default().as_str(), READY_INDEX_KEY);
}

/// An index reports the family it was built over, which is what tells a
/// suite's own keys apart from the production ones it must never touch.
#[tokio::test]
async fn an_index_reports_the_family_it_was_built_over() {
    let production = ReadyIndex::new(pending_connection());
    assert_eq!(production.prefix(), &ReadyPrefix::production());

    let private = ReadyPrefix::private("index-addressing");
    let isolated = ReadyIndex::under(pending_connection(), private.clone());
    assert_eq!(isolated.prefix(), &private);
    assert_ne!(
        isolated.prefix(),
        production.prefix(),
        "a private family that reads as the production one would let a suite \
         write into the index a deployment polls"
    );
}

/// A connection renders its role and its budget and stops there. It is the
/// line an operator reads beside a stuck command, and the driver's own
/// connection state is both unprintable and not theirs to act on.
#[tokio::test]
async fn a_connection_renders_its_role_and_budget_and_nothing_else() {
    let rendered = format!("{:?}", pending_connection());
    assert!(rendered.starts_with("Dragonfly"), "{rendered}");
    assert!(
        rendered.contains(&format!("{REQUEST_BUDGET:?}")),
        "the request budget is what a stuck command is read against: {rendered}"
    );
    assert!(
        rendered.contains(".."),
        "the rendering must stay non-exhaustive: {rendered}"
    );
}
