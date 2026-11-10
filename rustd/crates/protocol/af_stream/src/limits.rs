//! Validate a declared length before any allocation it controls.

use af_wire::protocol::MESSAGE_BYTES_MAX;
use garde::{Unvalidated, Valid, Validate};

use crate::Result;

#[derive(Debug, Validate)]
pub(crate) struct Length {
    #[garde(range(min = 1, max = MESSAGE_BYTES_MAX))]
    pub(crate) bytes: usize,
}

impl Length {
    pub(crate) fn prove(bytes: usize) -> Result<Valid<Self>> {
        Ok(Unvalidated::new(Self { bytes }).validate()?)
    }
}
