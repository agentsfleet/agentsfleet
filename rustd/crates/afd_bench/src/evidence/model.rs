//! Serialized shapes shared by capture and grading.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::datastores::DatastoreProbe;

/// Default checked-in capture plan.
pub const DEFAULT_PLAN_PATH: &str = "bench/profiles/datastore/redis-historical.json";

/// Root containing immutable datastore campaigns.
pub const CAMPAIGN_ROOT: &str = "bench/baselines/datastore";

/// Capture format version.
pub(super) const EVIDENCE_SCHEMA: u32 = 1;

/// One historical baseline campaign's required shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselinePlan {
    /// Serialized format version.
    pub schema: u32,
    /// Stable directory name under [`CAMPAIGN_ROOT`].
    pub campaign: String,
    /// Production revision the new capture must remain comparable to.
    pub baseline_revision: String,
    /// Profile all historical samples use.
    pub profile: String,
    /// Distinct captures required for every lane.
    pub samples_per_lane: u32,
    /// Lane names required, in capture order.
    pub lanes: Vec<String>,
}

/// Two digests whose equality is itself evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ProofPair {
    /// Digest at the baseline revision.
    pub baseline_sha256: String,
    /// Digest at the capture revision.
    pub capture_sha256: String,
    /// Explicit result, so a reader does not infer it from two long strings.
    pub equal: bool,
}

impl ProofPair {
    /// Build the pair and its equality result.
    pub(super) fn new(baseline_sha256: String, capture_sha256: String) -> Self {
        let equal = baseline_sha256 == capture_sha256;
        Self {
            baseline_sha256,
            capture_sha256,
            equal,
        }
    }
}

/// Source and dependency evidence shared by every sample.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Provenance {
    /// Serialized format version.
    pub schema: u32,
    /// Historical comparison revision.
    pub baseline_revision: String,
    /// Committed revision whose binaries produced the samples.
    pub capture_revision: String,
    /// Production Rust source, excluding `afd_bench`.
    pub production_source: ProofPair,
    /// Shipped schema files.
    pub schema_files: ProofPair,
    /// Production manifests and build inputs.
    pub production_build: ProofPair,
    /// Whole Cargo.lock bytes; allowed to differ for a bench-only dependency.
    pub cargo_lock: ProofPair,
    /// Cargo.lock with only the `afd_bench` package stanza removed.
    pub production_lock: ProofPair,
    /// Resolved dependencies and features reachable from production roots.
    pub production_dependency_closure: ProofPair,
    /// Every non-milestone changed path; the revisions retain the full delta.
    pub changed_paths: Vec<String>,
}

/// A value old drivers did not record, without inventing a zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Availability<T> {
    /// The driver recorded the value directly.
    Available { value: T },
    /// The historical driver did not produce it.
    Unavailable { reason: String },
}

/// Machine resources that make two runs comparable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resources {
    /// Operating system family.
    pub os: String,
    /// Processor architecture.
    pub architecture: String,
    /// Logical processors visible to the process.
    pub logical_cpus: usize,
    /// Host memory in bytes when the operating system exposes it.
    pub memory_bytes: Availability<u64>,
}

/// One archived sample and every proof needed to interpret it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sidecar {
    /// Serialized format version.
    pub schema: u32,
    /// Stable campaign directory name.
    pub campaign: String,
    /// Benchmark lane name.
    pub lane: String,
    /// One-based sample number within the lane.
    pub sample: u32,
    /// Safety and scale profile used by the driver.
    pub profile: String,
    /// Production revision used for comparability.
    pub baseline_revision: String,
    /// Committed revision whose binary produced this sample.
    pub capture_revision: String,
    /// Wall-clock capture time for audit order only.
    pub captured_at_unix_ms: u128,
    /// Every parameter the historical driver recorded.
    pub parameters: BTreeMap<String, u64>,
    /// Payload size, or an explicit historical limitation.
    pub payload_bytes: Availability<u64>,
    /// Measurement window, when the driver recorded one.
    pub window_seconds: Availability<u64>,
    /// Offered rate, or an explicit historical limitation.
    pub offered_rate_per_second: Availability<u64>,
    /// Generator seed, or an explicit historical limitation.
    pub seed: Availability<u64>,
    /// Host resources visible during capture.
    pub resources: Resources,
    /// Stable identity for the datastore topology across samples.
    pub topology_fingerprint_sha256: String,
    /// Raw connected-server and discovered-node evidence.
    pub datastore_probe: DatastoreProbe,
    /// Result path relative to the sample directory.
    pub result_file: String,
    /// Digest of the exact result bytes.
    pub result_sha256: String,
    /// Driver log path relative to the sample directory.
    pub raw_log_file: String,
    /// Digest of the exact driver log bytes.
    pub raw_log_sha256: String,
    /// Provenance path relative to the sample directory.
    pub provenance_file: String,
    /// Digest of the campaign provenance bytes.
    pub provenance_sha256: String,
    /// Digest of raw Postgres identity output.
    pub postgres_raw_sha256: String,
    /// Digest of raw Redis server output.
    pub redis_server_raw_sha256: String,
    /// Digest of raw Redis replication output.
    pub redis_replication_raw_sha256: String,
    /// Digest of raw Redis cluster topology output or refusal.
    pub redis_topology_raw_sha256: String,
}
