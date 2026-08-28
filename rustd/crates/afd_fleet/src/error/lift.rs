//! How a foreign error becomes this crate's.
//!
//! Six [`From`] impls, split from [`super`] because they answer one question
//! the constructors beside them do not: which failures `?` may lift with no
//! conversion written at the call site. That is a policy about the crate's
//! boundary, and `RUST_ERROR_STANDARD` rule 2 is what it implements — compose
//! with `#[from]` so `?` lifts, and reach for `map_err` only to ADD context the
//! call site alone knows.
//!
//! Two of these types are wrapped in more than one place, and the asymmetry is
//! deliberate rather than an oversight; each impl below says why.

use super::{Error, ErrorKind};

/// A pool with nothing to give, or a datastore that is gone.
///
/// `#[from]`, so `?` lifts an `afd_db::Error` with no conversion at the call
/// site (`RUST_ERROR_STANDARD` rule 2).
impl From<afd_db::Error> for Error {
    fn from(source: afd_db::Error) -> Self {
        Self::new(ErrorKind::Datastore { source })
    }
}

/// The queue would not answer.
///
/// A separate variant from [`ErrorKind::Datastore`] because the two fail
/// independently and a runner reads them the same way — back off and re-poll —
/// only when the code says which one went down. Folding Redis into the Postgres
/// variant would page whoever owns the wrong datastore.
impl From<afd_redis::Error> for Error {
    fn from(source: afd_redis::Error) -> Self {
        Self::new(ErrorKind::Queue { source })
    }
}

/// A money read or charge failed.
///
/// The billing store moved to its own crate with its own error, and the
/// admission pass still speaks THIS crate's: the posture that decides what a
/// money fault means lives here, so the fault composes in rather than the
/// posture moving out.
impl From<afd_billing::Error> for Error {
    fn from(source: afd_billing::Error) -> Self {
        Self::new(ErrorKind::Billing { source })
    }
}

/// An identifier could not be minted — the instant is unrepresentable.
///
/// `#[from]` on the KIND, lifted here, so `?` carries a `Uuid7::encode` failure
/// with no conversion at the call site. `RowMalformed` wraps the same foreign
/// type but keeps `#[source]` and its own builder, because a column that will
/// not parse needs the table and column names a bare conversion cannot supply —
/// and because two `#[from]` for one type is two `From` impls for one pair.
impl From<afd_core::error::Error> for Error {
    fn from(source: afd_core::error::Error) -> Self {
        Self::new(ErrorKind::Mint { source })
    }
}

/// The host could not produce the random bytes a credential is built from.
/// `#[from]` on the KIND, lifted here for the reason the two impls above are:
/// a fleet whose stored config will not parse must not run under a config this
/// daemon guessed at, and the parser's own error is what says which rule the
/// document broke. Converting it to a string here would destroy that.
impl From<afd_fleet_runtime::Error> for Error {
    fn from(source: afd_fleet_runtime::Error) -> Self {
        Self::new(ErrorKind::ConfigUnreadable { source })
    }
}

impl From<afd_crypto::error::Error> for Error {
    fn from(source: afd_crypto::error::Error) -> Self {
        Self::new(ErrorKind::Entropy { source })
    }
}
