//! Content-free terminal outcomes shared by model-call producers.
use super::closed_set;
closed_set! {
    /// The stream's terminal answer, before optional usage recovery.
    CallOutcome {
        /// The provider completed successfully.
        Completed => "completed",
        /// The provider or transport interrupted the stream.
        Provider => "provider_interrupted",
        /// The request exceeded its deadline.
        Deadline => "deadline",
        /// Its caller cancelled the stream.
        Cancelled => "cancelled"
    }
}
