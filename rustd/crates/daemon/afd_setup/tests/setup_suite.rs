//! Native setup boundary proofs, grouped by observable behavior.
#![expect(
    clippy::print_stderr,
    reason = "a suite's helpers print their measured evidence into the lane log"
)]

mod draft;
#[path = "../src/model/provider/tests/wire.rs"]
mod provider_wire;
