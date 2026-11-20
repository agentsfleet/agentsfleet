//! Initial turn, recovery and receipt confirmation share one derived identity.

mod admit;
mod confirm;
mod recover;
mod store;

use afd_turns::PreparedTurn;

pub(super) use store::{key, payload, store};

pub(super) enum Prepared<'a> {
    None,
    Ready(PreparedTurn<'a>),
    Blocked(String),
}
