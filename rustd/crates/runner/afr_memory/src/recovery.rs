//! The bounded local overlay; shared reads and permissions always come from a new seed.

use af_wire::memory::{INDEXED_TEXT_MAX_BYTES, MemoryDelta};
use garde::Validate;
use serde::{Deserialize, Deserializer, Serialize};

use crate::RECALL_MISS_CAP;
use crate::error::Result;
use crate::loaded::{PUSH_BUDGET_BYTES, push_bytes};

/// Private memory changes captured with the root conversation.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct MemoryRecovery {
    #[serde(deserialize_with = "owned_entries")]
    #[garde(dive, custom(push_budget))]
    pub(crate) pending: Vec<MemoryDelta<'static>>,
    #[garde(inner(length(bytes, min = 1, max = INDEXED_TEXT_MAX_BYTES), pattern(r"^[^\x00]+$")))]
    pub(crate) superseded: Vec<String>,
    #[garde(range(max = RECALL_MISS_CAP))]
    pub(crate) misses: usize,
}

impl std::fmt::Debug for MemoryRecovery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryRecovery")
            .field("pending_count", &self.pending.len())
            .field("hidden_count", &self.superseded.len())
            .field("recall_misses", &self.misses)
            .finish()
    }
}

impl MemoryRecovery {
    pub(crate) const fn new(
        pending: Vec<MemoryDelta<'static>>,
        superseded: Vec<String>,
        misses: usize,
    ) -> Self {
        Self {
            pending,
            superseded,
            misses,
        }
    }
}

fn owned_entries<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<MemoryDelta<'static>>, D::Error> {
    Ok(Vec::<MemoryDelta<'de>>::deserialize(deserializer)?
        .into_iter()
        .map(MemoryDelta::into_owned)
        .collect())
}

fn push_budget<C: ?Sized>(entries: &[MemoryDelta<'_>], _context: &C) -> garde::Result {
    if entries
        .iter()
        .map(push_bytes)
        .fold(0_usize, usize::saturating_add)
        > PUSH_BUDGET_BYTES
    {
        return Err(garde::Error::new("the memory overlay exceeds one push"));
    }
    Ok(())
}
