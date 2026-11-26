//! How a failure becomes an [`Error`](super::Error): the lifts, and the raisers
//! that bind what only the call site knows.

#[cfg(target_os = "linux")]
use afr_engine::EgressRefusal;

#[cfg(target_os = "linux")]
use super::Step;
use super::{Error, ErrorKind};

// Every lift is a `From`, so `?` does the conversion and no `map_err` appears
// on a path that adds nothing (`docs/RUST_ERROR_STANDARD.md` rule 2).
af_core::error_lifts!(Error, ErrorKind:
    std::io::Error => Io,
    afr_engine::Error => Seam,
);

/// Refuses a run whose egress cannot be held to its policy, saying why, in
/// the seam's words.
#[cfg(target_os = "linux")]
pub(crate) fn egress_refused(refusal: EgressRefusal) -> Error {
    afr_engine::error::egress_refused(refusal).into()
}

/// Names the netlink `step` when the kernel refuses it.
#[cfg(target_os = "linux")]
pub(crate) fn netlink(step: Step) -> impl Fn(std::io::Error) -> Error {
    move |source| ErrorKind::Netlink { step, source }.into()
}
