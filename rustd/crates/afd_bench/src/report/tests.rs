//! What the file says, and what it refuses to say.

#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use core::time::Duration;
use std::fs;
use std::path::{Path, PathBuf};

use super::{
    DatastoreCosts, Fixture, Lane, Latency, P95_MS, P99_MS, Provenance, RATE_PER_SECOND,
    RESULTS_DIRECTORY, Report,
};
use crate::fixture::{FixtureLedger, RunPrefix};
use crate::profile::Profile;

/// A directory that removes itself, so a failing test leaves no litter.
///
/// `tempfile` is not a workspace dependency and this is the only place that
/// would want one; a unique name under the system temp directory plus a `Drop`
/// is the whole of what it would provide here.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        let unique = format!(
            "afd-bench-{label}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).expect("a scratch directory must be creatable");
        Self { path }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// The fleet population the filled report describes, and the object count its
/// fixture therefore created and swept: one per fleet.
const FLEETS: u64 = 1_000;

/// A report carrying one of everything a lane reports.
fn filled_report() -> Report {
    let mut report = Report::new(Lane::Lease, Profile::Rig, Provenance::for_test());
    report.parameter("fleets", FLEETS);
    report.parameter("runners", 64);
    report.measurement(RATE_PER_SECOND, 812.5);
    report.measurement(P95_MS, 14.25);
    report.datastores = DatastoreCosts {
        dragonfly: super::DatastoreCost {
            operations: 4_000,
            time_ms: Some(120.0),
        },
        postgres: super::DatastoreCost {
            operations: 2_100,
            time_ms: Some(640.0),
        },
    };
    report.fixture = Fixture {
        run_prefix: "bench-1-2".to_owned(),
        created: FLEETS,
        swept: FLEETS,
    };
    report
}

#[test]
fn test_each_lane_writes_a_parseable_result() {
    let scratch = Scratch::new("parseable");

    for lane in Lane::ALL {
        let mut report = Report::new(lane, Profile::Rig, Provenance::for_test());
        report.measurement(RATE_PER_SECOND, 1.0);
        let path = scratch.join(&format!("{}.json", lane.name()));

        report
            .write(&path)
            .expect("a finished run writes its result");
        let read = Report::read(&path).expect("what a lane wrote, a rubric reads");

        assert_eq!(read.lane, lane);
        assert_eq!(read.profile, "rig");
        assert_eq!(read, report);
    }
}

#[test]
fn test_a_result_carries_every_block_a_reader_needs() {
    let scratch = Scratch::new("blocks");
    let path = scratch.join("lease.rig.json");

    filled_report().write(&path).expect("writable");
    let raw = fs::read_to_string(&path).expect("readable");
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");

    for block in [
        "lane",
        "profile",
        "parameters",
        "measurements",
        "datastores",
        "fixture",
    ] {
        assert!(
            parsed.get(block).is_some(),
            "the rubric greps {block} out of every result"
        );
    }
    assert_eq!(parsed["datastores"]["dragonfly"]["operations"], 4_000);
    assert_eq!(parsed["datastores"]["postgres"]["operations"], 2_100);
    assert_eq!(parsed["fixture"]["created"], parsed["fixture"]["swept"]);
}

#[test]
fn test_a_partial_write_never_lands_on_the_result_path() {
    let scratch = Scratch::new("atomic");
    let path = scratch.join("steer.rig.json");

    // A directory occupying the pending path makes the render land nowhere.
    let pending = path.with_extension("json.pending");
    fs::create_dir_all(&pending).expect("the obstruction must be creatable");

    let refused = filled_report()
        .write(&path)
        .expect_err("a write that cannot complete must not report success");

    assert!(!refused.is_pre_flight());
    assert!(
        !path.exists(),
        "the result path must be untouched when the run did not finish, so a \
         reader never quotes half a run"
    );
}

#[test]
fn test_an_unparseable_result_fails_the_comparison() {
    let scratch = Scratch::new("truncated");
    let path = scratch.join("lease.rig.json");
    fs::write(&path, "{\"lane\": \"lease\", \"profile\":").expect("writable");

    let refused = Report::read(&path).expect_err("a truncated result is not a measurement");

    assert!(
        refused.to_string().contains("lease.rig.json"),
        "the refusal must name the file, said: {refused}"
    );
}

#[test]
fn test_a_missing_result_names_the_file_it_wanted() {
    let scratch = Scratch::new("absent");

    let refused =
        Report::read(&scratch.join("never-written.json")).expect_err("an absent file is not a run");

    assert!(refused.to_string().contains("never-written.json"));
}

#[test]
fn test_the_latency_block_is_spelled_once_for_every_lane() {
    let mut latency = Latency::new().expect("buildable");
    for _ in 0..100 {
        latency
            .record(Duration::from_millis(10))
            .expect("recordable");
    }
    let mut report = Report::new(Lane::Outbound, Profile::Rig, Provenance::for_test());

    report.latency(2.0, &latency);

    assert!((report.measurements[RATE_PER_SECOND] - 50.0).abs() < f64::EPSILON);
    assert!(report.measurements.contains_key(P95_MS));
    assert!(report.measurements.contains_key(P99_MS));
}

#[test]
fn test_a_run_with_no_elapsed_time_reports_no_rate() {
    let latency = Latency::new().expect("buildable");
    let mut report = Report::new(Lane::Steer, Profile::Rig, Provenance::for_test());

    report.latency(0.0, &latency);

    assert!(
        !report.measurements.contains_key(RATE_PER_SECOND),
        "dividing by a zero window would report an infinite rate as a measurement"
    );
}

#[test]
fn test_a_lane_and_profile_decide_the_result_path() {
    let path = Lane::Lease.result_path(Profile::Dev);

    assert_eq!(path, Path::new(RESULTS_DIRECTORY).join("lease.dev.json"));
    assert_ne!(path, Lane::Lease.baseline_path(Profile::Dev));
}

#[test]
fn test_the_fixture_block_reads_off_the_ledger() {
    let prefix = RunPrefix::mint();
    let mut ledger = FixtureLedger::new();
    ledger.created(12);
    ledger.swept(12);

    let fixture = Fixture::of(&prefix, ledger);

    assert_eq!(fixture.run_prefix, prefix.as_str());
    assert_eq!(fixture.created, 12);
    assert_eq!(fixture.swept, 12);
}

#[test]
fn test_an_empty_distribution_reports_no_tail() {
    let latency = Latency::new().expect("buildable");
    let mut report = Report::new(Lane::Steer, Profile::Rig, Provenance::for_test());

    report.latency(2.0, &latency);

    for key in [P95_MS, P99_MS, super::MAX_MS] {
        assert!(
            !report.measurements.contains_key(key),
            "{key} on nothing recorded is a zero nobody measured"
        );
    }
}

#[test]
fn every_lane_parses_from_the_name_it_writes_its_files_under() {
    for lane in Lane::ALL {
        let parsed: Lane = lane.name().parse().expect("a lane's own name");
        assert_eq!(parsed, lane);
    }
}

#[test]
fn an_unknown_lane_is_refused_with_the_spellings_that_exist() {
    let refused = "leases".parse::<Lane>();

    let Some(crate::error::ErrorKind::UnknownLane { usage }) =
        refused.as_ref().err().map(crate::Error::kind)
    else {
        panic!("a near-miss spelling is not a lane: {refused:?}");
    };
    for lane in Lane::ALL {
        assert!(
            usage.contains(lane.name()),
            "{usage} must offer {}",
            lane.name()
        );
    }
}

#[test]
fn a_result_whose_directory_cannot_be_made_names_that_directory() {
    let scratch = Scratch::new("unmakeable");
    let occupied = scratch.join("a-file");
    fs::write(&occupied, "").expect("the blocking file");
    let path = occupied.join("lease.rig.json");

    let refused = filled_report().write(&path);

    let Some(crate::error::ErrorKind::ResultUnwritable { path: named, .. }) =
        refused.as_ref().err().map(crate::Error::kind)
    else {
        panic!("a file where the directory goes must refuse the write: {refused:?}");
    };
    assert_eq!(*named, occupied);
}

#[test]
fn a_result_that_cannot_land_names_its_destination_and_leaves_it_untouched() {
    let scratch = Scratch::new("unlandable");
    // A non-empty directory at the destination: the pending file writes
    // beside it, and only the rename can refuse.
    let path = scratch.join("lease.rig.json");
    fs::create_dir_all(path.join("occupant")).expect("the blocking directory");

    let refused = filled_report().write(&path);

    let Some(crate::error::ErrorKind::ResultUnwritable { path: named, .. }) =
        refused.as_ref().err().map(crate::Error::kind)
    else {
        panic!("a directory at the destination must refuse the rename: {refused:?}");
    };
    assert_eq!(*named, path);
    assert!(
        path.join("occupant").is_dir(),
        "a refused rename replaced nothing"
    );
}

#[test]
fn a_rate_over_no_time_is_zero_rather_than_infinite() {
    assert!(super::per_second(10, 0.0).abs() < f64::EPSILON);
    assert!(super::per_second(10, -1.0).abs() < f64::EPSILON);
    assert!((super::per_second(10, 2.0) - 5.0).abs() < f64::EPSILON);
}

#[test]
fn a_result_path_with_no_file_name_is_refused_rather_than_written_elsewhere() {
    let refused = filled_report().write(Path::new(""));

    assert!(
        matches!(
            refused.as_ref().err().map(crate::Error::kind),
            Some(crate::error::ErrorKind::ResultUnwritable { .. })
        ),
        "an empty path names no file to write: {refused:?}"
    );
}
