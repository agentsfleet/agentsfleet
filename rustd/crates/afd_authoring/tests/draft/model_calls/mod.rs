//! The physical provider boundary is the only fake; identity, vault and receipts are real.

mod arithmetic;
mod authorization;
mod bounds;
mod compaction;
mod concurrency;
mod corrections;
mod counting;
mod deadline;
mod fixture;
mod lapse;
mod late_results;
mod observation;
mod output_bounds;
mod preamble;
mod progress;
mod provider_failures;
mod reasoning;
mod reasoning_safety;
mod receipt_races;
mod recovery;
mod renewal;
mod replay;
mod reports;
mod text_edits;
mod uncertainty;
