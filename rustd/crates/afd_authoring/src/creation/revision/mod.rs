//! Improvement application owns one transaction for the setup pointer and replay.

mod apply;
mod persist;
mod result;

use super::source::Captured;
use crate::{Result, error::ErrorKind, types::SourcePin};
use afd_core::id::Uuid7;

fn target(captured: &Captured, fleet: &Uuid7, current: i64) -> Result<()> {
    match &captured.context.source_pin {
        Some(SourcePin::Fleet {
            fleet_id,
            setup_revision,
        }) if fleet_id == fleet && setup_revision == &current.to_string() => Ok(()),
        _ => Err(ErrorKind::State {
            current: "source_revision_changed",
        }
        .into()),
    }
}
