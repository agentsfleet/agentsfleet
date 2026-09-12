//! Git and Cargo proofs for one committed capture revision.

use std::process::{Command, Output};

use sha2::{Digest as _, Sha256};

use self::metadata::dependency_closure;
use super::model::{CAMPAIGN_ROOT, EVIDENCE_SCHEMA, ProofPair, Provenance};
use crate::error::{Error, Result};

mod metadata;

const GIT_COMMAND: &str = "git";
const LOCK_PATH: &str = "rustd/Cargo.lock";
const CRATES_PATH: &str = "rustd/crates/";
const BENCH_CRATE_PATH: &str = "rustd/crates/afd_bench/";
const OUTBOUND_OWNERSHIP_SEAM_PATH: &str = "rustd/crates/afd_outbound/src/poster.rs";
const PACKAGE_STANZA: &str = "[[package]]";
const DIFF: &str = "diff";

/// SHA-256 of bytes, rendered in the form every sidecar uses.
pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

/// Resolve a revision to the full object identifier.
pub(crate) fn revision(name: &str) -> Result<String> {
    git(
        &["rev-parse", "--verify", &format!("{name}^{{commit}}")],
        "git revision",
    )
    .map(|raw| raw.trim().to_owned())
}

/// Refuse capture when working bytes could differ from the named revision.
pub(super) fn require_capture_tree(campaign: &str) -> Result<()> {
    let result = output(
        Command::new(GIT_COMMAND).args(["status", "--porcelain=v1", "-z", "--untracked-files=all"]),
        "git status",
    )?;
    if capture_only(&result.stdout, campaign) {
        return Ok(());
    }
    Err(invalid(
        "capture worktree differs from HEAD outside its evidence campaign",
    ))
}

fn capture_only(status: &[u8], campaign: &str) -> bool {
    let allowed = format!("{CAMPAIGN_ROOT}/{campaign}/");
    status
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .all(|entry| {
            entry
                .get(3..)
                .is_some_and(|path| path.starts_with(allowed.as_bytes()))
        })
}

/// Build all B/B0 source, schema, build and resolved-dependency proofs.
pub(crate) fn provenance(baseline: &str, capture: &str) -> Result<Provenance> {
    let baseline = revision(baseline)?;
    let capture = revision(capture)?;
    let production_source = surface(&baseline, &capture, Surface::Source)?;
    let outbound_ownership_seam_sha256 = digest(
        git(
            &[
                DIFF,
                "--binary",
                &baseline,
                &capture,
                "--",
                OUTBOUND_OWNERSHIP_SEAM_PATH,
            ],
            "outbound ownership seam diff",
        )?
        .as_bytes(),
    );
    let schema_files = surface(&baseline, &capture, Surface::Schema)?;
    let production_build = surface(&baseline, &capture, Surface::Build)?;
    let baseline_lock = file_at(&baseline, LOCK_PATH)?;
    let capture_lock = file_at(&capture, LOCK_PATH)?;
    let cargo_lock = ProofPair::new(digest(&baseline_lock), digest(&capture_lock));
    let production_lock = ProofPair::new(
        digest(&without_bench_lock(&baseline_lock)),
        digest(&without_bench_lock(&capture_lock)),
    );
    let production_dependency_closure = ProofPair::new(
        digest(&dependency_closure(&baseline)?),
        digest(&dependency_closure(&capture)?),
    );
    let changed_paths = git(&[DIFF, "--name-only", &baseline, &capture], "git diff")?
        .lines()
        // Milestone filenames are documentation labels, not runtime inputs,
        // and the repository's production-source audit forbids those labels
        // in generated evidence. Their bytes remain covered by Git revisions.
        .filter(|path| !path.starts_with("docs/v2/"))
        .map(str::to_owned)
        .collect();
    Ok(Provenance {
        schema: EVIDENCE_SCHEMA,
        baseline_revision: baseline,
        capture_revision: capture,
        production_source,
        outbound_ownership_seam_sha256,
        schema_files,
        production_build,
        cargo_lock,
        production_lock,
        production_dependency_closure,
        changed_paths,
    })
}

/// A tree surface whose byte identity must survive bench-only changes.
#[derive(Clone, Copy)]
enum Surface {
    Source,
    Schema,
    Build,
}

fn surface(baseline: &str, capture: &str, surface: Surface) -> Result<ProofPair> {
    Ok(ProofPair::new(
        digest(&tree_listing(baseline, surface)?),
        digest(&tree_listing(capture, surface)?),
    ))
}

fn tree_listing(revision: &str, surface: Surface) -> Result<Vec<u8>> {
    let listing = git(&["ls-tree", "-r", "--full-tree", revision], "git tree")?;
    let selected = listing
        .lines()
        .filter(|line| {
            line.split_once('\t')
                .is_some_and(|(_, path)| selected(path, surface))
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(selected.into_bytes())
}

fn selected(path: &str, surface: Surface) -> bool {
    match surface {
        Surface::Source => {
            path.starts_with(CRATES_PATH)
                && path.contains("/src/")
                && !path.starts_with(BENCH_CRATE_PATH)
                && path != OUTBOUND_OWNERSHIP_SEAM_PATH
        }
        Surface::Schema => path.starts_with("schema/"),
        Surface::Build => {
            matches!(
                path,
                "Dockerfile"
                    | "docker-compose.yml"
                    | "docs/metrics.census.tsv"
                    | "rustd/Cargo.toml"
                    | "rustd/rust-toolchain.toml"
            ) || (path.starts_with(CRATES_PATH)
                && (path.ends_with("/Cargo.toml") || path.ends_with("/build.rs"))
                && !path.starts_with(BENCH_CRATE_PATH))
        }
    }
}

pub(super) fn file_at(revision: &str, path: &str) -> Result<Vec<u8>> {
    output(
        Command::new(GIT_COMMAND).args(["show", &format!("{revision}:{path}")]),
        "git show",
    )
    .map(|result| result.stdout)
}

/// Remove only the workspace package stanza whose dependencies are bench-only.
fn without_bench_lock(raw: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(raw);
    let mut kept = Vec::new();
    for (index, section) in text.split(PACKAGE_STANZA).enumerate() {
        if index == 0 || !section.lines().any(|line| line == "name = \"afd_bench\"") {
            kept.push(section);
        }
    }
    kept.join(PACKAGE_STANZA).into_bytes()
}

fn git(args: &[&str], operation: &'static str) -> Result<String> {
    let result = output(Command::new(GIT_COMMAND).args(args), operation)?;
    Ok(String::from_utf8_lossy(&result.stdout).into_owned())
}

fn output(command: &mut Command, operation: &'static str) -> Result<Output> {
    let result = command
        .output()
        .map_err(|source| Error::EvidenceCommand { operation, source })?;
    if !result.status.success() {
        return Err(invalid(&format!(
            "{operation} exited {}: {}",
            result.status,
            String::from_utf8_lossy(&result.stderr).trim()
        )));
    }
    Ok(result)
}

pub(super) fn invalid(detail: &str) -> Error {
    Error::EvidenceInvalid(detail.to_owned())
}

#[cfg(test)]
mod tests;
