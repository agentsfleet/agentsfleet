//! Every `afd_billing` datastore test, in one test binary.
//!
//! Each test seeds its own tenant, workspace and fleet, and every statement
//! under test carries one of them in its predicate, so the tests run in
//! parallel with nothing to race on. The lane module is declared once and
//! reached as `crate::lane`.
#![cfg(feature = "test-util")]

#[path = "billing/lane.rs"]
mod lane;

#[path = "billing/accounts.rs"]
mod accounts;
#[path = "billing/lock_wait.rs"]
mod lock_wait;
#[path = "billing/reads.rs"]
mod reads;
#[path = "billing/spend.rs"]
mod spend;
