//! Exact regular-file layout for one immutable evidence campaign.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::invalid;
use crate::error::{Error, Result};
use crate::evidence::model::BaselinePlan;

const PROVENANCE_FILE: &str = "provenance.json";
const RESULT_FILE: &str = "result.json";
const RAW_LOG_FILE: &str = "raw.log";
const SIDECAR_FILE: &str = "sidecar.json";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    File,
    Directory,
}

pub(super) fn validate(campaign: &Path, plan: &BaselinePlan) -> Result<()> {
    let mut campaign_entries = vec![(PROVENANCE_FILE.to_owned(), Kind::File)];
    campaign_entries.extend(
        plan.lanes
            .iter()
            .cloned()
            .map(|lane| (lane, Kind::Directory)),
    );
    require_entries(campaign, campaign_entries)?;

    for lane in &plan.lanes {
        let lane_directory = campaign.join(lane);
        let samples = (1..=plan.samples_per_lane)
            .map(|sample| (sample_directory(sample), Kind::Directory))
            .collect();
        require_entries(&lane_directory, samples)?;
        for sample in 1..=plan.samples_per_lane {
            require_entries(
                &lane_directory.join(sample_directory(sample)),
                vec![
                    (RAW_LOG_FILE.to_owned(), Kind::File),
                    (RESULT_FILE.to_owned(), Kind::File),
                    (SIDECAR_FILE.to_owned(), Kind::File),
                ],
            )?;
        }
    }
    Ok(())
}

fn sample_directory(sample: u32) -> String {
    format!("sample-{sample:02}")
}

fn require_entries(directory: &Path, expected: Vec<(String, Kind)>) -> Result<()> {
    let root_kind = fs::symlink_metadata(directory).map_err(|source| Error::ResultUnreadable {
        path: directory.to_path_buf(),
        source,
    })?;
    if !root_kind.file_type().is_dir() {
        return Err(invalid("evidence directory is not a regular directory"));
    }

    let expected: BTreeMap<_, _> = expected.into_iter().collect();
    let mut actual = BTreeMap::new();
    for entry in fs::read_dir(directory).map_err(|source| Error::ResultUnreadable {
        path: directory.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| Error::ResultUnreadable {
            path: directory.to_path_buf(),
            source,
        })?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_name| invalid("evidence entry name is not UTF-8"))?;
        let file_type = entry
            .file_type()
            .map_err(|source| Error::ResultUnreadable {
                path: entry.path(),
                source,
            })?;
        let kind = if file_type.is_file() {
            Kind::File
        } else if file_type.is_dir() {
            Kind::Directory
        } else {
            return Err(invalid("evidence tree contains a symlink or special file"));
        };
        actual.insert(name, kind);
    }
    if actual != expected {
        return Err(invalid("evidence directory members are not exact"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
