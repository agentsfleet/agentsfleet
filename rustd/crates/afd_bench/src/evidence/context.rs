//! Host, topology, and historical-driver context recorded in each sidecar.

use std::collections::BTreeMap;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use super::git::digest;
use super::model::{Availability, BaselinePlan, EVIDENCE_SCHEMA, Provenance, Resources, Sidecar};
use crate::datastores::DatastoreProbe;
use crate::report::{Lane, Report};

pub(super) const RESULT_FILE: &str = "result.json";
pub(super) const RAW_LOG_FILE: &str = "raw.log";
const BYTES_PER_KIBIBYTE: u64 = 1024;

#[derive(Clone, Copy)]
pub(super) struct Inputs<'a> {
    pub plan: &'a BaselinePlan,
    pub proof: &'a Provenance,
    pub lane: Lane,
    pub sample: u32,
    pub report: &'a Report,
    pub probe: &'a DatastoreProbe,
    pub result_raw: &'a [u8],
    pub raw_log: &'a [u8],
    pub provenance_raw: &'a [u8],
}

pub(super) fn sidecar(input: Inputs<'_>) -> Sidecar {
    let Inputs {
        plan,
        proof,
        lane,
        sample,
        report,
        probe,
        result_raw,
        raw_log,
        provenance_raw,
    } = input;
    Sidecar {
        schema: EVIDENCE_SCHEMA,
        campaign: plan.campaign.clone(),
        lane: lane.name().to_owned(),
        sample,
        profile: plan.profile.clone(),
        baseline_revision: proof.baseline_revision.clone(),
        capture_revision: proof.capture_revision.clone(),
        captured_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        parameters: report.parameters.clone(),
        payload_bytes: unavailable("historical driver did not record payload bytes"),
        window_seconds: report
            .parameters
            .get("BENCH_WINDOW_SECONDS")
            .copied()
            .map_or_else(
                || unavailable("historical driver has no measured window"),
                available,
            ),
        offered_rate_per_second: unavailable(
            "historical driver was closed-loop and did not record offered rate",
        ),
        seed: unavailable("historical driver did not accept a deterministic seed"),
        resources: resources(),
        topology_fingerprint_sha256: topology_fingerprint(probe),
        datastore_probe: probe.clone(),
        result_file: RESULT_FILE.to_owned(),
        result_sha256: digest(result_raw),
        raw_log_file: RAW_LOG_FILE.to_owned(),
        raw_log_sha256: digest(raw_log),
        provenance_file: "../../provenance.json".to_owned(),
        provenance_sha256: digest(provenance_raw),
        postgres_raw_sha256: digest(probe.postgres_raw.as_bytes()),
        redis_server_raw_sha256: digest(probe.redis_server_raw.as_bytes()),
        redis_replication_raw_sha256: digest(probe.redis_replication_raw.as_bytes()),
        redis_topology_raw_sha256: digest(probe.redis_topology_raw.as_bytes()),
    }
}

fn resources() -> Resources {
    Resources {
        os: std::env::consts::OS.to_owned(),
        architecture: std::env::consts::ARCH.to_owned(),
        logical_cpus: std::thread::available_parallelism().map_or(1, usize::from),
        memory_bytes: memory_bytes().map_or_else(
            || unavailable("operating system did not expose host memory"),
            available,
        ),
    }
}

fn memory_bytes() -> Option<u64> {
    if cfg!(target_os = "macos") {
        return Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()
            .filter(|result| result.status.success())
            .and_then(|result| String::from_utf8(result.stdout).ok())
            .and_then(|raw| raw.trim().parse().ok());
    }
    fs::read_to_string("/proc/meminfo")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()
        .and_then(|kilobytes| kilobytes.checked_mul(BYTES_PER_KIBIBYTE))
}

pub(super) fn topology_fingerprint(probe: &DatastoreProbe) -> String {
    let mut stable = BTreeMap::new();
    stable.insert("postgres", probe.postgres_raw.clone());
    stable.insert("hosts", probe.discovered_hosts.join(","));
    stable.insert("topology", probe.redis_topology_raw.clone());
    for (name, raw) in [
        ("server", probe.redis_server_raw.as_str()),
        ("replication", probe.redis_replication_raw.as_str()),
    ] {
        let fields = stable_info(raw);
        stable.insert(name, fields);
    }
    digest(&serde_json::to_vec(&stable).unwrap_or_default())
}

fn stable_info(raw: &str) -> String {
    raw.lines()
        .filter(|line| {
            [
                "redis_version:",
                "redis_mode:",
                "tcp_port:",
                "role:",
                "master_host:",
                "master_port:",
                "connected_slaves:",
            ]
            .iter()
            .any(|prefix| line.starts_with(prefix))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn available<T>(value: T) -> Availability<T> {
    Availability::Available { value }
}

fn unavailable<T>(reason: &str) -> Availability<T> {
    Availability::Unavailable {
        reason: reason.to_owned(),
    }
}
