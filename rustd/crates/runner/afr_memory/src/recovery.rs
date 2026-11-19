//! The bounded local overlay; shared reads and permissions always come from a new seed.

use af_wire::memory::{MAX_ENTRIES_PER_FLEET, MAX_KEY_LEN, MAX_PUSH_BYTES, MemoryDelta};
use garde::Validate;
use serde::{Deserialize, Deserializer, Serialize};

use crate::RECALL_MISS_CAP;
use crate::error::Result;

/// Maximum keys hidden by a running turn, including keys outside hydration.
const MAX_HIDDEN_KEYS: usize = 4 * MAX_ENTRIES_PER_FLEET;

/// Private memory changes captured with the root conversation.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct MemoryRecovery {
    #[serde(deserialize_with = "owned_entries")]
    #[garde(length(max = MAX_ENTRIES_PER_FLEET), dive, custom(push_budget))]
    pub(crate) pending: Vec<MemoryDelta<'static>>,
    #[garde(length(max = MAX_HIDDEN_KEYS), inner(length(bytes, min = 1, max = MAX_KEY_LEN), pattern(r"^[^\x00]+$")))]
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
    if entries.iter().map(MemoryDelta::bytes).sum::<usize>() > MAX_PUSH_BYTES {
        return Err(garde::Error::new("the memory overlay exceeds one push"));
    }
    Ok(())
}
