//! Content-free settlement survives every private-content cleanup.

use super::{PRIVATE_TTL, State, cipher, files};
use crate::{Result, error};
use afd_wire::authoring::{Accounting, Report};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::time::Instant;

#[derive(Serialize, Deserialize)]
pub(super) struct Settlement {
    pub(super) lease_id: String,
    pub(super) attempt_id: String,
    pub(super) accounting: Accounting,
}

#[derive(Serialize, Deserialize)]
pub(super) struct Metadata {
    pub(super) lease_id: String,
    pub(super) attempt_id: String,
    pub(super) report_id: String,
    pub(super) generation: String,
    #[serde(with = "afd_wire::instant")]
    pub(super) first_written_at: Timestamp,
    #[serde(with = "afd_wire::instant")]
    pub(super) expires_at: Timestamp,
}

pub(super) fn settlement(directory: &Path) -> Result<Settlement> {
    serde_json::from_slice(&files::read(&directory.join(files::ACCOUNTING), 8192)?)
        .map_err(error::malformed(crate::client::Verb::AuthoringReport))
}

pub(super) fn clean(
    directory: &Path,
    state: &mut State,
    now: Timestamp,
    certain: bool,
) -> Result<()> {
    files::clear_unknown(directory)?;
    if !certain || valid(directory, state, now).is_err() {
        files::clear_private(directory)?;
        state.deadlines.remove(directory);
    }
    Ok(())
}

fn valid(
    directory: &Path,
    state: &mut State,
    now: Timestamp,
) -> Result<(Metadata, afd_crypto::secret::Kek)> {
    let (metadata, key) = cipher::metadata(directory)?;
    let accounting = settlement(directory)?;
    let lease = directory.file_name().and_then(|name| name.to_str());
    if lease != Some(metadata.lease_id.as_str())
        || accounting.lease_id != metadata.lease_id
        || accounting.attempt_id != metadata.attempt_id
        || accounting.accounting.report_id != metadata.report_id
        || accounting.accounting.deletion_generation != metadata.generation
        || metadata.expires_at != afd_core::clock::after(metadata.first_written_at, PRIVATE_TTL)
        || now < metadata.first_written_at
        || now >= metadata.expires_at
    {
        return Err(error::config(
            "private report metadata is expired or inconsistent",
        ));
    }
    let remaining = crate::deadline::left(now, metadata.expires_at);
    let deadline = state
        .deadlines
        .entry(PathBuf::from(directory))
        .or_insert_with(|| Instant::now() + remaining);
    if Instant::now() >= *deadline {
        return Err(error::config(
            "private report reached its monotonic deadline",
        ));
    }
    Ok((metadata, key))
}

pub(super) fn load(
    directory: &Path,
    state: &mut State,
    now: Timestamp,
    certain: bool,
) -> Result<(String, Report)> {
    clean(directory, state, now, certain)?;
    let settlement = settlement(directory)?;
    let answer = if certain {
        if let Ok(answer) = valid(directory, state, now)
            .and_then(|(metadata, key)| cipher::open(directory, &metadata, &key))
        {
            serde_json::from_str::<Option<String>>(&answer).ok()
        } else {
            files::clear_private(directory)?;
            state.deadlines.remove(directory);
            None
        }
    } else {
        None
    };
    let report = match answer {
        Some(answer) => Report::Terminal {
            accounting: settlement.accounting,
            answer,
        },
        None => Report::Accounting {
            accounting: settlement.accounting,
        },
    };
    Ok((settlement.lease_id, report))
}
