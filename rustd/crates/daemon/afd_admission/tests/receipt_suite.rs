//! Real ledger behavior over an in-process bus, with a dead or silent one for failure injection.
#![cfg(feature = "test-util")]

mod receipts;
