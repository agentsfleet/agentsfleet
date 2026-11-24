//! An edit names the schedule's own fields, and nothing else.
//!
//! Every schedule is an independent row, so an `override` body has no recipe
//! under it to overlay: it is refused rather than read as a field edit, which
//! would silently apply a body written for a different shape.

use crate::{DesiredStatus, Result, Schedule, error::ErrorKind, validate};
use afd_api_wire::schedule::{Patch, Status};

/// The schedule's intent once an edit is applied, every field validated.
pub(super) struct Replacement {
    pub(super) cron: String,
    pub(super) timezone: String,
    pub(super) message: String,
    pub(super) status: Status,
}

/// What `row` becomes under `patch`: each field the patch names, the row's
/// own value for every other.
///
/// # Errors
/// `Patch` for an `override` body or a body that names no field; `Invalid`
/// for a merged expression, zone or message the grammar refuses.
pub(super) fn replacement(row: &Schedule, patch: &Patch) -> Result<Replacement> {
    let named = patch.cron.is_some()
        || patch.timezone.is_some()
        || patch.message.is_some()
        || patch.status.is_some();
    if patch.overrides.is_some() || !named {
        return Err(ErrorKind::Patch.into());
    }
    let current = if row.desired_status == DesiredStatus::Paused {
        Status::Paused
    } else {
        Status::Active
    };
    let replaced = Replacement {
        cron: patch.cron.clone().unwrap_or_else(|| row.cron.clone()),
        timezone: patch
            .timezone
            .clone()
            .unwrap_or_else(|| row.timezone.clone()),
        message: patch.message.clone().unwrap_or_else(|| row.message.clone()),
        status: patch.status.unwrap_or(current),
    };
    validate::Fields {
        expression: Some(&replaced.cron),
        timezone: Some(&replaced.timezone),
        message: Some(&replaced.message),
    }
    .check()
    .map_err(|invalid| ErrorKind::Invalid { invalid })?;
    Ok(replaced)
}
