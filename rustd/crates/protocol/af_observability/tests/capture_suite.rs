//! Families whose values a test reads back through the capturing exporter.
//!
//! # Why its own binary
//!
//! `producers::install` and `Capture::install` both write process-wide
//! `OnceLock`s, and `test_util` says a binary installing its own provider may
//! not share with a suite that installs another. `producers_drive.rs` installs
//! a plain provider and asserts no value; this one installs the capture and
//! asserts values, so the two cannot be the same binary.
#![cfg(feature = "test-util")]

#[path = "billing_capture.rs"]
mod billing_capture;

#[path = "task_capture.rs"]
mod task_capture;
