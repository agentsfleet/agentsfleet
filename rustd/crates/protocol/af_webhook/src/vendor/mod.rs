//! Verifiers derived from a vendor's own implementation.
//!
//! Everything under here carries a provenance header naming the upstream
//! repository, release, commit and licence, plus the list of local patches
//! applied. That header is the point: a signature verifier is where a vendor's
//! accumulated protocol knowledge lives, and a copy with no lineage cannot be
//! diffed when the vendor ships a fix.
//!
//! A file lands here only when the vendor's crate cannot be depended on
//! directly, and the reason is recorded in the file rather than remembered.

pub mod svix;
