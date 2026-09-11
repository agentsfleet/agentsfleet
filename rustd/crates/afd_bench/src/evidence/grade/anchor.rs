//! Git-backed immutability for an otherwise writable evidence directory.

use std::path::Path;

use super::invalid;
use crate::error::Result;
use crate::evidence::capture::read;
use crate::evidence::git::{file_at, revision};
use crate::evidence::model::{BaselinePlan, CAMPAIGN_ROOT};

const PROVENANCE_FILE: &str = "provenance.json";
const SAMPLE_FILES: [&str; 3] = ["raw.log", "result.json", "sidecar.json"];

pub(super) fn validate(plan: &BaselinePlan, campaign: &Path) -> Result<()> {
    let evidence_revision = plan
        .evidence_revision
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid("capture plan has no immutable evidence revision"))?;
    if revision(evidence_revision)? != evidence_revision {
        return Err(invalid(
            "capture plan evidence revision is not a full commit identifier",
        ));
    }

    require_committed(plan, campaign, PROVENANCE_FILE, evidence_revision)?;
    for lane in &plan.lanes {
        for sample in 1..=plan.samples_per_lane {
            for file in SAMPLE_FILES {
                let relative = format!("{lane}/sample-{sample:02}/{file}");
                require_committed(plan, campaign, &relative, evidence_revision)?;
            }
        }
    }
    Ok(())
}

fn require_committed(
    plan: &BaselinePlan,
    campaign: &Path,
    relative: &str,
    evidence_revision: &str,
) -> Result<()> {
    let archived = read(&campaign.join(relative))?;
    let repository_path = format!("{CAMPAIGN_ROOT}/{}/{relative}", plan.campaign);
    let committed = file_at(evidence_revision, &repository_path)?;
    if archived != committed {
        return Err(invalid(&format!(
            "{relative} differs from immutable evidence revision"
        )));
    }
    Ok(())
}
