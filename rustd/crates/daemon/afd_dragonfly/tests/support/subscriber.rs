//! The tracing subscriber for the binaries that path-include `fake_redis`.
//!
//! `fake_redis/mod.rs` re-exports `crate::subscriber::install_subscriber`, and
//! `afd_admission` and `afd_fleet` include both files by path, so the name stays
//! here while the one implementation is `afd_core`'s.

pub(crate) use afd_core::test_util::trace::install_subscriber;
