//! Supervisor-only native checkpoint resources.

pub(crate) mod authorize;
pub(crate) mod commit;
pub(crate) mod objects_read;
pub(crate) mod objects_register;
pub(crate) mod page_read;
pub(crate) mod page_write;
pub(crate) mod pin;
pub(crate) mod plan;
pub(crate) mod uploads;

use afd_http::handler::{Refusal, read_strict_body};

const EVENT: &str = "runner_checkpoint_operation_failed";
const MALFORMED: &str = "Checkpoint request exceeds its supported shape or bounds";

fn input<T: serde::de::DeserializeOwned + garde::Validate<Context = ()>>(
    bytes: &[u8],
) -> Result<garde::Valid<T>, Refusal> {
    let value: T = read_strict_body(bytes).map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    garde::Unvalidated::new(value)
        .validate()
        .map_err(|_invalid| Refusal::malformed(MALFORMED))
}
