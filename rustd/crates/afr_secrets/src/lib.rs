//! The runner's secrets, and the one masker every output passes.
//!
//! A [`Secret`] is a token held in memory: the runner's own, or one minted for
//! a lease. [`StaticSecrets`] is a lease's `secrets_map`, read one way.
//! [`Scrub`] masks every known secret value as `«secret:NAME»` before text
//! reaches a frame, the trace, a record, the model or the report; the egress
//! guard masks the tokens it mints with the same type, so there is one masker
//! (`docs/architecture/runner_execution.md` §Credentials).

pub mod error;
pub mod json;

mod scrub;
mod secret;
mod statics;

pub use self::error::{Error, Result};
pub use self::scrub::{Carry, Clean, Scrub};
pub use self::secret::Secret;
pub use self::statics::{FIELD_HOST, FIELD_TOKEN, StaticSecrets};
