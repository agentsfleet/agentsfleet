//! The one way this workspace builds an HTTP client that must reach its
//! endpoint directly.
//!
//! The key-set fetch, the claim read, the metadata write, a vendor's token
//! exchange and the scheduler's calls each carry a credential or decide who
//! may act. An environment proxy (`HTTPS_PROXY`, `ALL_PROXY`) silently
//! rerouting one of them is not a convenience worth having, so every such
//! client starts here and a change to that rule lands once.

use std::time::Duration;

/// A client builder bounded by `timeout` that ignores every environment proxy.
///
/// A builder rather than a built client, so a caller that also bounds its
/// redirects says so before building, and each caller keeps mapping a build
/// failure into its own error.
pub fn direct(timeout: Duration) -> reqwest::ClientBuilder {
    // Every deadline in this workspace is set where the call is made; the
    // caller names it, this function only refuses to go without one.
    reqwest::Client::builder().timeout(timeout).no_proxy()
}
