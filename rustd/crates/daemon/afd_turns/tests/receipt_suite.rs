//! Real ledger behavior over the lane's NATS, or over an in-process bus where a
//! case reads the wake rings, with a dead or silent one for failure injection.
#![cfg(feature = "test-util")]

mod receipts;
