//! How a foreign error becomes this crate's.
//!
//! Split from [`super`] because it answers one question the constructors beside
//! it do not: which failures `?` may lift with no conversion written at the
//! call site. That is a policy about the crate's boundary, and
//! `RUST_ERROR_STANDARD` rule 2 is what it implements — compose with `From` so
//! `?` lifts, and reach for `map_err` only to ADD context the call site alone
//! knows.
//!
//! Each source lands in its own kind. Most are planes that moved to their own
//! crate — admission, billing, credentials, gates, memory, events, outbound —
//! while the lease still speaks THIS crate's error, so their faults compose in
//! with the code and sentence their own plane decided.
//!
//! Two foreign types are wrapped in more than one place, and the asymmetry is
//! deliberate: `afd_core::error::Error` lifts into `Mint` here, while
//! `RowMalformed` keeps `#[source]` and its own builder, because a column that
//! will not parse needs the table and column names a bare conversion cannot
//! supply — and two lifts for one type is two `From` impls for one pair.
//! `afd_fleet_runtime::Error` lifts whole into `ConfigUnreadable`: a fleet whose
//! stored config will not parse must not run under a config this daemon
//! guessed at, and the parser's own error says which rule the document broke.
//!
//! `afd_db` and `afd_dragonfly` stay separate kinds because the two datastores
//! fail independently, and a runner backs off the same way from either only
//! when the code says which one went down.

use super::{Error, ErrorKind};

afd_core::error_lifts!(Error, ErrorKind:
    afd_authoring::Error => Authoring,
    afd_db::Error => Datastore,
    afd_runner::Error => Runner,
    afd_dragonfly::Error => Queue,
    afd_admission::Error => Admission,
    afd_gate::Error => Gate,
    afd_credential::Error => Credential,
    afd_billing::Error => Billing,
    afd_memory::Error => Memory,
    afd_events::Error => Events,
    afd_outbound::Error => Outbound,
    afd_core::error::Error => Mint,
    afd_fleet_runtime::Error => ConfigUnreadable,
    afd_crypto::error::Error => Entropy,
    afr_secrets::Error => Scrub,
);
