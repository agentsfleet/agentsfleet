//! The runner plane's verbs.
//!
//! One module per verb, because a verb is the unit a reviewer reads: the Zig
//! daemon keeps one file per handler for the same reason, and the split
//! survives the port unchanged. What does not survive is the shape INSIDE each
//! file — a Zig handler holds a request context, acquires a connection, runs
//! statements and writes a response, so there is no seam between deciding and
//! answering and no way to test one without the other.
//!
//! Here the decision is `afd_fleet`'s and the answer is this crate's. Every
//! function below is short enough to read in one sitting for that reason, not
//! by discipline.
//!
//! # Enrolment is on the OTHER plane
//!
//! [`enrolment`] answers `POST /v1/runners`, which is a tenant operator
//! creating a runner — `Guard::Bearer` and `runner:enroll`, not a runner token.
//! It lives here anyway because it writes the row every verb beside it reads,
//! and splitting it away from the plane it creates would put the mint and the
//! thing it mints for in two different places.

pub(crate) mod enrolment;
pub(crate) mod heartbeat;
pub(crate) mod self_record;
