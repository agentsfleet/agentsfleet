//! Fail-closed validation for a complete immutable benchmark campaign.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::capture::{parse_json, plan, read, require_comparable, sidecar_file};
use super::context::topology_fingerprint;
use super::git::{digest, provenance};
use super::model::{
    Availability, BaselinePlan, CAMPAIGN_ROOT, EVIDENCE_SCHEMA, Provenance, Resources, Sidecar,
};
use crate::error::{Error, Result};
use crate::fixture::RunPrefix;
use crate::report::{Lane, Report};

mod anchor;
mod tree;

const RESULT_ROLE: &str = "result";
const RAW_LOG_ROLE: &str = "raw log";

/// Successful campaign counts printed by `make bench-datastore`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grade {
    /// Lanes represented by the campaign.
    pub lanes: usize,
    /// Immutable samples validated.
    pub samples: usize,
}

/// Validate every byte and cross-sample invariant in a historical campaign.
///
/// # Errors
///
/// Refuses missing, extra, changed, inconsistent, or incomparable evidence;
/// also returns source-control, metadata, and filesystem failures.
pub fn grade(plan_path: impl AsRef<Path>) -> Result<Grade> {
    grade_at(plan_path, Path::new(CAMPAIGN_ROOT))
}

/// Grade a campaign rooted at an explicit path for repository fixture tests.
pub(super) fn grade_at(
    plan_path: impl AsRef<Path>,
    campaign_root: impl AsRef<Path>,
) -> Result<Grade> {
    let plan = plan(plan_path)?;
    let campaign = campaign_root.as_ref().join(&plan.campaign);
    let proof_path = campaign.join("provenance.json");
    let proof_raw = read(&proof_path)?;
    let proof: Provenance = parse_json(&proof_path, &proof_raw)?;
    validate_provenance(&plan, &proof)?;
    tree::validate(&campaign, &plan)?;
    anchor::validate(&plan, &campaign)?;

    let expected = plan.lanes.len() * usize::try_from(plan.samples_per_lane).unwrap_or(0);
    let mut resources: Option<Resources> = None;
    let mut topology: Option<String> = None;
    let mut run_prefixes = BTreeSet::new();
    for lane in Lane::ALL {
        let mut parameters = None;
        let mut captures = BTreeSet::new();
        let mut captured_after = 0;
        for sample in 1..=plan.samples_per_lane {
            let directory = campaign
                .join(lane.name())
                .join(format!("sample-{sample:02}"));
            let (sidecar, run_prefix) =
                validate_sample(&plan, &proof, &proof_raw, lane, sample, &directory)?;
            require_unique_prefix(&mut run_prefixes, run_prefix)?;
            require_distinct_capture(
                &mut captures,
                &mut captured_after,
                sidecar.captured_at_unix_ms,
                &sidecar.result_sha256,
                &sidecar.raw_log_sha256,
            )?;
            require_same(
                "machine resources",
                &mut resources,
                sidecar.resources.clone(),
            )?;
            require_same(
                "datastore topology",
                &mut topology,
                sidecar.topology_fingerprint_sha256.clone(),
            )?;
            require_same("lane parameters", &mut parameters, sidecar.parameters)?;
        }
    }
    Ok(Grade {
        lanes: plan.lanes.len(),
        samples: expected,
    })
}

pub(super) fn require_unique_prefix(
    run_prefixes: &mut BTreeSet<String>,
    run_prefix: String,
) -> Result<()> {
    if !run_prefixes.insert(run_prefix) {
        return Err(invalid("campaign samples reuse a benchmark run prefix"));
    }
    Ok(())
}

pub(super) fn require_distinct_capture(
    captures: &mut BTreeSet<(String, String)>,
    captured_after: &mut u128,
    captured_at: u128,
    result_digest: &str,
    raw_log_digest: &str,
) -> Result<()> {
    if captured_at <= *captured_after
        || !captures.insert((result_digest.to_owned(), raw_log_digest.to_owned()))
    {
        return Err(invalid(
            "lane samples are duplicated or outside capture order",
        ));
    }
    *captured_after = captured_at;
    Ok(())
}

fn validate_provenance(plan: &BaselinePlan, stored: &Provenance) -> Result<()> {
    if stored.schema != EVIDENCE_SCHEMA || stored.baseline_revision != plan.baseline_revision {
        return Err(invalid(
            "campaign provenance does not match its capture plan",
        ));
    }
    require_comparable(stored)?;
    if stored.changed_paths.iter().any(|path| !allowed_delta(path)) {
        return Err(invalid(
            "capture revision contains a delta outside benchmark evidence and governance",
        ));
    }
    let regenerated = provenance(&stored.baseline_revision, &stored.capture_revision)?;
    if regenerated != *stored {
        return Err(invalid(
            "campaign provenance no longer matches its Git revisions",
        ));
    }
    Ok(())
}

pub(super) fn allowed_delta(path: &str) -> bool {
    path == ".github/workflows/test-integration-rustd.yml"
        || path == ".github/workflows/test.yml"
        || path == ".oracle/orly.json"
        || path == "AGENTS.md"
        || path == "dispatch/write_spec.md"
        || path == "make/bench.mk"
        || path == "playbooks/README.md"
        || path == "rustd/Cargo.lock"
        || path.starts_with("bench/")
        || path.starts_with("docs/")
        || path.starts_with("playbooks/operations/datastore_scaling/")
        || path.starts_with("rustd/crates/afd_bench/")
}

fn validate_sample(
    plan: &BaselinePlan,
    proof: &Provenance,
    proof_raw: &[u8],
    lane: Lane,
    sample: u32,
    directory: &Path,
) -> Result<(Sidecar, String)> {
    let sidecar_path = directory.join(sidecar_file());
    let sidecar: Sidecar = parse_json(&sidecar_path, &read(&sidecar_path)?)?;
    validate_sidecar_identity(plan, proof, lane, sample, &sidecar)?;
    validate_availability(&sidecar)?;

    let result_path = safe_member(directory, &sidecar.result_file, RESULT_ROLE)?;
    let log_path = safe_member(directory, &sidecar.raw_log_file, RAW_LOG_ROLE)?;
    let result_raw = read(&result_path)?;
    let log_raw = read(&log_path)?;
    if log_raw.is_empty() {
        return Err(invalid("captured raw log is empty"));
    }
    require_digest(RESULT_ROLE, &sidecar.result_sha256, &result_raw)?;
    require_digest(RAW_LOG_ROLE, &sidecar.raw_log_sha256, &log_raw)?;
    require_digest("provenance", &sidecar.provenance_sha256, proof_raw)?;
    validate_probe(&sidecar)?;

    let report: Report = parse_json(&result_path, &result_raw)?;
    if report.lane != lane
        || report.profile != plan.profile
        || report.parameters != sidecar.parameters
        || !report.created
        || report.abort.is_some()
        || report.fixture.swept < report.fixture.created
    {
        return Err(invalid(
            "archived report is incomplete or contradicts its sidecar",
        ));
    }
    validate_log(&plan.profile, lane, &report.fixture.run_prefix, &log_raw)?;
    Ok((sidecar, report.fixture.run_prefix))
}

pub(super) fn validate_log(profile: &str, lane: Lane, run_prefix: &str, raw: &[u8]) -> Result<()> {
    RunPrefix::existing(run_prefix)?;
    let log = core::str::from_utf8(raw).map_err(|_source| invalid("raw log is not UTF-8"))?;
    for expected in [
        format!("→ [bench-{}] profile={profile}", lane.name()),
        format!("run_prefix={run_prefix}"),
        format!("wrote bench/results/{}.{profile}.json", lane.name()),
    ] {
        if log.lines().filter(|line| *line == expected).count() != 1 {
            return Err(invalid("raw log does not identify its archived run"));
        }
    }
    Ok(())
}

fn validate_sidecar_identity(
    plan: &BaselinePlan,
    proof: &Provenance,
    lane: Lane,
    sample: u32,
    sidecar: &Sidecar,
) -> Result<()> {
    if sidecar.schema != EVIDENCE_SCHEMA
        || sidecar.campaign != plan.campaign
        || sidecar.lane != lane.name()
        || sidecar.sample != sample
        || sidecar.profile != plan.profile
        || sidecar.baseline_revision != proof.baseline_revision
        || sidecar.capture_revision != proof.capture_revision
        || sidecar.captured_at_unix_ms == 0
        || sidecar.provenance_file != "../../provenance.json"
    {
        return Err(invalid("sidecar identity does not match its campaign slot"));
    }
    Ok(())
}

fn validate_availability(sidecar: &Sidecar) -> Result<()> {
    for value in [
        erased(&sidecar.payload_bytes),
        erased(&sidecar.window_seconds),
        erased(&sidecar.offered_rate_per_second),
        erased(&sidecar.seed),
        erased(&sidecar.resources.memory_bytes),
    ] {
        if value.is_some_and(str::is_empty) {
            return Err(invalid("an unavailable field has no reason"));
        }
    }
    Ok(())
}

fn erased<T>(value: &Availability<T>) -> Option<&str> {
    match value {
        Availability::Available { .. } => None,
        Availability::Unavailable { reason } => Some(reason.trim()),
    }
}

fn validate_probe(sidecar: &Sidecar) -> Result<()> {
    let probe = &sidecar.datastore_probe;
    if probe.postgres_raw.is_empty()
        || probe.redis_server_raw.is_empty()
        || probe.redis_replication_raw.is_empty()
        || probe.redis_topology_raw.is_empty()
    {
        return Err(invalid("raw datastore probe is incomplete"));
    }
    for (name, expected, raw) in [
        (
            "Postgres probe",
            &sidecar.postgres_raw_sha256,
            probe.postgres_raw.as_bytes(),
        ),
        (
            "Redis server probe",
            &sidecar.redis_server_raw_sha256,
            probe.redis_server_raw.as_bytes(),
        ),
        (
            "Redis replication probe",
            &sidecar.redis_replication_raw_sha256,
            probe.redis_replication_raw.as_bytes(),
        ),
        (
            "Redis topology probe",
            &sidecar.redis_topology_raw_sha256,
            probe.redis_topology_raw.as_bytes(),
        ),
    ] {
        require_digest(name, expected, raw)?;
    }
    if topology_fingerprint(probe) != sidecar.topology_fingerprint_sha256 {
        return Err(invalid("datastore topology fingerprint changed"));
    }
    Ok(())
}

fn safe_member(directory: &Path, name: &str, role: &str) -> Result<PathBuf> {
    let path = Path::new(name);
    if path.components().count() != 1 || path.is_absolute() {
        return Err(invalid(&format!(
            "{role} path escapes its sample directory"
        )));
    }
    Ok(directory.join(path))
}

fn require_digest(role: &str, expected: &str, raw: &[u8]) -> Result<()> {
    if digest(raw) != expected {
        return Err(invalid(&format!("{role} digest does not match its bytes")));
    }
    Ok(())
}

fn require_same<T: PartialEq>(role: &str, expected: &mut Option<T>, value: T) -> Result<()> {
    if let Some(first) = expected {
        if first != &value {
            return Err(invalid(&format!("{role} differs across samples")));
        }
    } else {
        *expected = Some(value);
    }
    Ok(())
}

fn invalid(detail: &str) -> Error {
    Error::EvidenceInvalid(detail.to_owned())
}
