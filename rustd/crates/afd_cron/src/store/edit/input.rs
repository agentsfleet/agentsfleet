//! An inherited overlay is replaced as a whole; Restore reads only its base.

use crate::{DesiredStatus, Inherited, Recipe, Result, Schedule, error::ErrorKind, validate};
use afd_api_wire::schedule::{Override, Patch, Status};

pub(super) fn replacement(row: &Schedule, patch: &Patch) -> Result<(Recipe, Option<Inherited>)> {
    let independent = patch.cron.is_some()
        || patch.timezone.is_some()
        || patch.message.is_some()
        || patch.status.is_some();
    let (effective, inherited) = if let Some(mut base) = row.recipe.clone() {
        let overlay = patch.overrides.clone().ok_or(ErrorKind::Patch)?;
        if independent || overlay.as_ref().is_some_and(empty) {
            return Err(ErrorKind::Patch.into());
        }
        base.overrides = overlay;
        base.override_revision = base
            .override_revision
            .checked_add(1)
            .ok_or(ErrorKind::Exhausted)?;
        (base.effective(), Some(base))
    } else {
        if patch.overrides.is_some() || !independent {
            return Err(ErrorKind::Patch.into());
        }
        let status = if row.desired_status == DesiredStatus::Paused {
            Status::Paused
        } else {
            Status::Active
        };
        (
            Recipe {
                cron: patch.cron.clone().unwrap_or_else(|| row.cron.clone()),
                timezone: patch
                    .timezone
                    .clone()
                    .unwrap_or_else(|| row.timezone.clone()),
                message: patch.message.clone().unwrap_or_else(|| row.message.clone()),
                status: patch.status.unwrap_or(status),
            },
            None,
        )
    };
    validate::Fields {
        expression: Some(&effective.cron),
        timezone: Some(&effective.timezone),
        message: Some(&effective.message),
    }
    .check()
    .map_err(|invalid| ErrorKind::Invalid { invalid })?;
    Ok((effective, inherited))
}

fn empty(overlay: &Override) -> bool {
    overlay.cron.is_none()
        && overlay.timezone.is_none()
        && overlay.message.is_none()
        && overlay.status.is_none()
}
