//! Private read custody is derived from a locked native wait, never a runner URL.

use crate::lease::scope::MintScope;

pub(crate) struct ReadPermit {
    pub(super) scope: MintScope,
    pub(super) installation: u64,
    pub(super) deadline: jiff::Timestamp,
}

impl ReadPermit {
    pub(crate) const fn scope(&self) -> &MintScope { &self.scope }
    pub(crate) const fn installation(&self) -> u64 { self.installation }
    pub(crate) const fn deadline(&self) -> jiff::Timestamp { self.deadline }
}
