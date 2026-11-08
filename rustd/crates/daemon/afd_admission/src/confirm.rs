//! Activation confirms committed acceptance by its immutable producer key.

use crate::{Admissions, Producer, Result};

/// The committed admission's identity. No second datastore acknowledges it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirmation {
    /// The event admitted under this producer key.
    pub event_id: String,
}

impl Admissions {
    /// Read the committed row and ring its hint. Never holds a lock across publish.
    ///
    /// # Errors
    /// Preserves database failures. A missing row answers `None`.
    pub async fn confirm(&self, producer: Producer, key: &str) -> Result<Option<Confirmation>> {
        let Some(row) = self.find_repeated(producer, key).await? else {
            return Ok(None);
        };
        self.wake(&row.fleet).await;
        Ok(Some(Confirmation { event_id: row.id }))
    }
}
