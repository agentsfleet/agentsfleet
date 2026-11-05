//! Native financial lifecycle scenarios use real rows, credit and vault keys.

mod accounting;
mod authority;
mod boundaries;
mod budgets;
mod concurrency;
mod diagnostics;
mod fixture;
mod funding;
mod maintenance;
mod maintenance_backlog;
mod maintenance_cancellation;
mod maintenance_fairness;
mod maintenance_races;
mod ordering;
mod outages;
mod probes;
mod recovery;
mod reply;
mod source;
mod source_capacity;
mod timeouts;
mod unpriced;
mod unusable;
