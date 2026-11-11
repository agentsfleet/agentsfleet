//! Native loopback I/O and bounded application framing share one test binary.

mod carrier_bounds;
mod failures;
mod framing;
mod ip_identity;
#[cfg(feature = "test-util")]
mod mtu;
mod native;
mod support;
