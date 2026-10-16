//! Capability hashes confer authority only alongside every current lease binding.

use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq as _;

use super::ModelCallInput;
use crate::{Result, error::ErrorKind, lease::guard::Attempt};

pub(super) fn check(attempt: &Attempt, request: &garde::Valid<ModelCallInput>) -> Result<()> {
    let presented = hex::encode(Sha256::digest(request.capability_token.as_bytes()));
    if attempt.id.as_str() != request.attempt_id
        || !bool::from(
            attempt
                .capability_hash
                .as_bytes()
                .ct_eq(presented.as_bytes()),
        )
    {
        return Err(ErrorKind::Fence.into());
    }
    Ok(())
}
