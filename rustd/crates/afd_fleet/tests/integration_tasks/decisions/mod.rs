//! Native financial lifecycle scenarios use real rows, credit and vault keys.

mod accounting;
mod budgets;
mod concurrency;
mod diagnostics;
mod fixture;
mod maintenance;
mod maintenance_backlog;
mod maintenance_cancellation;
mod maintenance_fairness;
mod maintenance_races;
mod ordering;
mod recovery;
mod timeouts;
