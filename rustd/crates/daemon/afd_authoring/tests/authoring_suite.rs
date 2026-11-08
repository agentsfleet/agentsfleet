//! Native setup boundary proofs, grouped by observable behavior.
#![expect(
    clippy::print_stderr,
    reason = "a suite's helpers print their measured evidence into the lane log"
)]

mod draft;
mod provider_wire;
