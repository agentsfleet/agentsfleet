//! The GitHub deliveries this crate's webhook handlers parse, captured from
//! github.com and embedded here, so a sibling crate's suite drives the same
//! bytes as this crate's own.

/// A failed run: the delivery that wakes a fleet.
pub const GITHUB_RUN_FAILURE: &str =
    include_str!("../tests/fixtures/webhooks/github_run_failure.json");

/// A failed run as an App sends it: the installation rides in the envelope.
pub const GITHUB_RUN_FAILURE_APP: &str =
    include_str!("../tests/fixtures/webhooks/github_run_failure_app.json");

/// The same run, green.
pub const GITHUB_RUN_SUCCESS: &str =
    include_str!("../tests/fixtures/webhooks/github_run_success.json");

/// A `ping`: the delivery GitHub sends when a hook is first created.
pub const GITHUB_PING: &str = include_str!("../tests/fixtures/webhooks/github_ping.json");

/// A pull-request delivery with every field github.com populates, its action
/// the placeholder `__ACTION__`.
pub const GITHUB_PULL_REQUEST: &str =
    include_str!("../tests/fixtures/webhooks/github_pull_request.json");

/// A terminal production deployment, as an App sends it.
pub const GITHUB_DEPLOYMENT_STATUS_APP: &str =
    include_str!("../tests/fixtures/webhooks/github_deployment_status_app.json");
