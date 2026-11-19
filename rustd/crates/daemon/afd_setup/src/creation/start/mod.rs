//! Initial admission, recovery and receipt confirmation share one derived identity.

mod admit;
mod confirm;
mod recover;
mod store;

use afd_admission::PreparedAdmission;

pub(super) use store::{key, payload, store};

pub(super) enum Prepared<'a> {
    None,
    Ready(PreparedAdmission<'a>),
    Blocked(String),
}
