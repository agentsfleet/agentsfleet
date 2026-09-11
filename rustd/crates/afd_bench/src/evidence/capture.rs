//! Create immutable campaign provenance and archive one completed sample.

use std::fs;
use std::path::{Path, PathBuf};

use super::context::{Inputs, RAW_LOG_FILE, RESULT_FILE, sidecar};
use super::git::{provenance, require_capture_tree};
use super::model::{BaselinePlan, CAMPAIGN_ROOT, EVIDENCE_SCHEMA, Provenance};
use crate::datastores::DatastoreProbe;
use crate::error::{Error, Result};
use crate::profile::Profile;
use crate::report::{Lane, Report};

const PROVENANCE_FILE: &str = "provenance.json";
const SIDECAR_FILE: &str = "sidecar.json";
const HEAD_REVISION: &str = "HEAD";

/// Load and validate the checked-in historical capture plan.
pub(super) fn plan(path: impl AsRef<Path>) -> Result<BaselinePlan> {
    let path = path.as_ref();
    let raw = read(path)?;
    let plan: BaselinePlan =
        serde_json::from_slice(&raw).map_err(|source| Error::ResultUnparseable {
            path: path.to_path_buf(),
            source,
        })?;
    validate_plan(&plan)?;
    Ok(plan)
}

/// Bind the campaign to a committed capture revision before any lane runs.
///
/// # Errors
///
/// Refuses an invalid plan, incomparable revisions, or provenance already
/// bound to another capture revision; also returns filesystem/tool failures.
pub fn prepare(path: impl AsRef<Path>) -> Result<PathBuf> {
    let plan = plan(path)?;
    require_capture_tree(&plan.campaign)?;
    let directory = campaign_directory(&plan);
    let target = directory.join(PROVENANCE_FILE);
    let proof = provenance(&plan.baseline_revision, HEAD_REVISION)?;
    require_comparable(&proof)?;
    if target.exists() {
        let stored: Provenance = parse_json(&target, &read(&target)?)?;
        if stored == proof {
            return Ok(target);
        }
        return Err(invalid(
            "campaign provenance already exists for another revision",
        ));
    }
    fs::create_dir_all(&directory).map_err(|source| Error::ResultUnwritable {
        path: directory,
        source,
    })?;
    write_json(&target, &proof)?;
    Ok(target)
}

/// Copy a fixed lane result into its one immutable campaign sample directory.
///
/// # Errors
///
/// Refuses an invalid or occupied sample slot, a mismatched report, changed
/// provenance, or an incomplete fixture sweep; also returns file errors.
pub fn capture(
    plan_path: impl AsRef<Path>,
    lane: Lane,
    sample: u32,
    raw_log_path: impl AsRef<Path>,
    probe: &DatastoreProbe,
) -> Result<PathBuf> {
    let plan = plan(plan_path)?;
    require_capture_tree(&plan.campaign)?;
    validate_slot(&plan, lane, sample)?;
    let provenance_path = campaign_directory(&plan).join(PROVENANCE_FILE);
    let provenance_raw = read(&provenance_path)?;
    let proof: Provenance = parse_json(&provenance_path, &provenance_raw)?;
    validate_provenance(&plan, &proof)?;

    let profile: Profile = plan.profile.parse()?;
    let result_path = lane.result_path(profile);
    let result_raw = read(&result_path)?;
    let report: Report = parse_json(&result_path, &result_raw)?;
    validate_report(&plan, lane, &report)?;
    let raw_log = read(raw_log_path.as_ref())?;

    let final_directory = sample_directory(&plan, lane, sample);
    refuse_existing(&final_directory)?;
    let pending = final_directory.with_extension("pending");
    refuse_existing(&pending)?;
    let parent = final_directory
        .parent()
        .ok_or_else(|| invalid("sample directory has no parent"))?;
    fs::create_dir_all(parent).map_err(|source| Error::ResultUnwritable {
        path: parent.to_path_buf(),
        source,
    })?;
    // `create_dir` reserves this exact slot atomically. Concurrent capture
    // processes must never share a pending directory and race to publish it.
    fs::create_dir(&pending).map_err(|source| Error::ResultUnwritable {
        path: pending.clone(),
        source,
    })?;

    fs::write(pending.join(RESULT_FILE), &result_raw).map_err(|source| {
        Error::ResultUnwritable {
            path: pending.join(RESULT_FILE),
            source,
        }
    })?;
    fs::write(pending.join(RAW_LOG_FILE), &raw_log).map_err(|source| Error::ResultUnwritable {
        path: pending.join(RAW_LOG_FILE),
        source,
    })?;
    let sidecar = sidecar(Inputs {
        plan: &plan,
        proof: &proof,
        lane,
        sample,
        report: &report,
        probe,
        result_raw: &result_raw,
        raw_log: &raw_log,
        provenance_raw: &provenance_raw,
    });
    write_json(&pending.join(SIDECAR_FILE), &sidecar)?;
    fs::rename(&pending, &final_directory).map_err(|source| Error::ResultUnwritable {
        path: final_directory.clone(),
        source,
    })?;
    Ok(final_directory)
}

pub(super) fn validate_plan(plan: &BaselinePlan) -> Result<()> {
    if plan.schema != EVIDENCE_SCHEMA
        || !safe_campaign(&plan.campaign)
        || plan.profile != "rig"
        || plan.samples_per_lane == 0
    {
        return Err(invalid("capture plan has an unsafe or unsupported shape"));
    }
    let expected = Lane::ALL.map(|lane| lane.name().to_owned()).to_vec();
    if plan.lanes != expected {
        return Err(invalid(
            "capture plan does not name every lane once in canonical order",
        ));
    }
    Ok(())
}

fn safe_campaign(campaign: &str) -> bool {
    !campaign.is_empty()
        && campaign.len() <= 96
        && campaign
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn validate_slot(plan: &BaselinePlan, lane: Lane, sample: u32) -> Result<()> {
    if !plan.lanes.iter().any(|name| name == lane.name()) {
        return Err(invalid("sample lane is absent from the capture plan"));
    }
    if sample == 0 || sample > plan.samples_per_lane {
        return Err(invalid("sample number is outside the capture plan"));
    }
    Ok(())
}

fn validate_provenance(plan: &BaselinePlan, proof: &Provenance) -> Result<()> {
    if proof.schema != EVIDENCE_SCHEMA || proof.baseline_revision != plan.baseline_revision {
        return Err(invalid("campaign provenance does not match its plan"));
    }
    let current = super::git::revision(HEAD_REVISION)?;
    if proof.capture_revision != current {
        return Err(invalid("HEAD changed after campaign preparation"));
    }
    require_comparable(proof)
}

pub(crate) fn require_comparable(proof: &Provenance) -> Result<()> {
    if !proof.production_source.equal
        || !proof.schema_files.equal
        || !proof.production_build.equal
        || !proof.production_lock.equal
        || !proof.production_dependency_closure.equal
    {
        return Err(invalid(
            "baseline and capture revisions differ outside the benchmark harness",
        ));
    }
    Ok(())
}

fn validate_report(plan: &BaselinePlan, lane: Lane, report: &Report) -> Result<()> {
    if report.lane != lane || report.profile != plan.profile {
        return Err(invalid(
            "fixed result path contains another lane or profile",
        ));
    }
    if !report.created || report.abort.is_some() || report.fixture.swept < report.fixture.created {
        return Err(invalid(
            "sample did not finish with a complete fixture sweep",
        ));
    }
    Ok(())
}

fn campaign_directory(plan: &BaselinePlan) -> PathBuf {
    Path::new(CAMPAIGN_ROOT).join(&plan.campaign)
}

pub(crate) fn sample_directory(plan: &BaselinePlan, lane: Lane, sample: u32) -> PathBuf {
    campaign_directory(plan)
        .join(lane.name())
        .join(format!("sample-{sample:02}"))
}

pub(crate) const fn sidecar_file() -> &'static str {
    SIDECAR_FILE
}

fn refuse_existing(path: &Path) -> Result<()> {
    if path.exists() {
        return Err(invalid(&format!(
            "{} already exists and cannot be overwritten",
            path.display()
        )));
    }
    Ok(())
}

pub(crate) fn read(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|source| Error::ResultUnreadable {
        path: path.to_path_buf(),
        source,
    })
}

pub(crate) fn parse_json<T: serde::de::DeserializeOwned>(path: &Path, raw: &[u8]) -> Result<T> {
    serde_json::from_slice(raw).map_err(|source| Error::ResultUnparseable {
        path: path.to_path_buf(),
        source,
    })
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let mut raw =
        serde_json::to_vec_pretty(value).map_err(|source| Error::ResultUnrenderable { source })?;
    raw.push(b'\n');
    fs::write(path, raw).map_err(|source| Error::ResultUnwritable {
        path: path.to_path_buf(),
        source,
    })
}

fn invalid(detail: &str) -> Error {
    Error::EvidenceInvalid(detail.to_owned())
}
