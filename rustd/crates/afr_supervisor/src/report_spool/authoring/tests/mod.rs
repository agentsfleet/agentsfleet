//! Exercise the same spool API used by startup, workers and online draining.
#![expect(clippy::unwrap_used, reason = "filesystem fixtures must fail loudly")]

mod cleanup;
mod delivery;
mod integrity;
mod refusal;
mod retention;

use super::{Spool, files};
use crate::storage_home::StorageHome;
use afd_core::{
    clock::{Clock, FixedClock},
    id::Uuid7,
};
use afd_wire::authoring::{Accounting, Knowledge, Meter, Outcome, Report};
use std::sync::Arc;

const LEASE: &str = "01890a5d-ac96-774b-bcce-b302099a8060";
const ATTEMPT: &str = "01890a5d-ac96-774b-bcce-b302099a8061";
const REPORT: &str = "01890a5d-ac96-774b-bcce-b302099a8062";
const ANSWER: &str = "private authoring answer that must expire";
const START: jiff::Timestamp = jiff::Timestamp::constant(1_700_000_000, 0);

struct Fixture {
    _root: tempfile::TempDir,
    home: StorageHome,
    clock: Arc<FixedClock>,
    spool: Spool,
    directory: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let home = StorageHome::open(root.path()).unwrap();
        let clock = Arc::new(FixedClock::at(START));
        let source: Arc<dyn Clock> = Arc::<FixedClock>::clone(&clock);
        let spool = Spool::open(&home, source).unwrap();
        let directory = home.spool().join("authoring").join(LEASE);
        Self {
            _root: root,
            home,
            clock,
            spool,
            directory,
        }
    }

    async fn hold(&self) {
        self.spool
            .hold(&Uuid7::parse(LEASE).unwrap(), ATTEMPT, report())
            .await
            .unwrap();
    }

    async fn read(&self) -> Report {
        let (lease, report) = self.spool.load(self.directory.clone()).await.unwrap();
        assert_eq!(lease, LEASE);
        report
    }

    fn restart(&mut self) {
        let clock: Arc<dyn Clock> = Arc::<FixedClock>::clone(&self.clock);
        self.spool = Spool::open(&self.home, clock).unwrap();
    }

    fn assert_accounting_only(&self, report: &Report) {
        assert!(matches!(report, Report::Accounting { .. }));
        assert_eq!(report.accounting().report_id, REPORT);
        assert_eq!(report.accounting().usage.runtime_ms, 100);
        let files: Vec<_> = std::fs::read_dir(&self.directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(files, [std::ffi::OsString::from(files::ACCOUNTING)]);
    }
}

fn report() -> Report {
    Report::Terminal {
        accounting: Accounting {
            fence: "1".to_owned(),
            report_id: REPORT.to_owned(),
            outcome: Outcome::Completed,
            failure_reason: None,
            usage: Meter {
                knowledge: Knowledge::Known,
                calls: 0,
                input_tokens: Some(0),
                output_tokens: Some(0),
                runtime_ms: 100,
                provider_cost_nanos: Some("0".to_owned()),
            },
        },
        answer: Some(ANSWER.to_owned()),
    }
}
