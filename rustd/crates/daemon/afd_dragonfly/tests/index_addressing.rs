//! Connection diagnostics exclude driver internals.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};

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
