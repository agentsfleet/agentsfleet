//! Host polling and its two authority-separated placement payloads.

mod kinds;
mod limits;
mod payload;
mod request;
mod response;

pub use kinds::{AcceptedKinds, LeaseKind};
pub use limits::{
    SANDBOX_CPU_MILLIS_MAX, SANDBOX_CPU_MILLIS_MIN, SANDBOX_DISK_BYTES_MAX, SANDBOX_DISK_BYTES_MIN,
    SANDBOX_MEMORY_BYTES_MAX, SANDBOX_MEMORY_BYTES_MIN, SandboxLimits,
};
pub use payload::{
    ANSWER_FAILED, ANSWER_FAILED_END, ANSWER_NONE, HISTORY_BYTES_MAX, HISTORY_TURNS_MAX,
    LeasePayload, SecretDelivery, TURN_TEXT_BYTES_MAX, Turn,
};
pub use request::LeaseRequest;
pub use response::{LeaseResponse, TaggedLease};
