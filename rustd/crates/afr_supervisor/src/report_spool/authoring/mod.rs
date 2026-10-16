//! Encrypted private reports expire independently of durable accounting replay.

mod cipher;
mod drain;
mod entry;
mod files;

use crate::{Result, error, storage_home::StorageHome};
use afd_core::{clock::Clock, id::Uuid7};
use afd_wire::authoring::{ENVELOPE_BYTES_MAX, Report};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::{sync::Notify, time::Instant};

const PRIVATE_TTL_MS: i64 = 24 * 60 * 60 * 1000;
const DIRECTORY: &str = "authoring";
/// The refusal a poisoned spool lock earns: a panic interrupted an update.
const STATE_INTERRUPTED: &str = "private report state was interrupted";

#[derive(garde::Validate)]
struct Envelope<'a> {
    #[garde(length(bytes, max = ENVELOPE_BYTES_MAX))]
    json: &'a str,
}

#[derive(Debug, Default)]
struct State {
    high_water: i64,
    uncertain: bool,
    deadlines: HashMap<PathBuf, Instant>,
}

/// Shared by the worker's first durable write and the startup/online drainer.
#[derive(Debug, Clone)]
pub(crate) struct Spool(Arc<Inner>);

#[derive(Debug)]
struct Inner {
    directory: PathBuf,
    clock: Arc<dyn Clock>,
    state: Mutex<State>,
    changed: Notify,
}

impl Spool {
    pub(crate) fn open(home: &StorageHome, clock: Arc<dyn Clock>) -> Result<Self> {
        let directory = home.spool().join(DIRECTORY);
        let spool = Self(Arc::new(Inner {
            directory,
            clock,
            state: Mutex::new(State::default()),
            changed: Notify::new(),
        }));
        spool.recover_clock()?;
        drain::report_held(spool.scan()?.held);
        Ok(spool)
    }

    /// Restores the clock high-water mark a previous process left.
    ///
    /// When it left none, every entry already on disk is uncertain. The state
    /// lock is released on return, before `scan` takes it again.
    fn recover_clock(&self) -> Result<()> {
        files::directory(&self.0.directory)?;
        let reading = files::read(&self.0.directory.join(files::CLOCK), 64);
        let old = reading
            .as_ref()
            .ok()
            .and_then(|bytes| serde_json::from_slice::<i64>(bytes).ok());
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_poisoned| error::config(STATE_INTERRUPTED))?;
        state.high_water = old.unwrap_or_else(|| self.0.clock.now().as_millis());
        state.uncertain = old.is_none() && fs::read_dir(&self.0.directory)?.count() != 0;
        Ok(())
    }

    pub(crate) async fn hold(&self, lease: &Uuid7, attempt: &str, report: Report) -> Result<()> {
        let report = garde::Unvalidated::new(report).validate()?.into_inner();
        let encoded =
            zeroize::Zeroizing::new(serde_json::to_string(&report).map_err(error::encode)?);
        garde::Unvalidated::new(Envelope { json: &encoded }).validate()?;
        let spool = self.clone();
        let lease = lease.clone();
        let attempt = Uuid7::parse(attempt)?;
        tokio::task::spawn_blocking(move || spool.write(&lease, &attempt, report)).await??;
        self.0.changed.notify_one();
        Ok(())
    }

    fn write(&self, lease: &Uuid7, attempt: &Uuid7, report: Report) -> Result<()> {
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_poisoned| error::config(STATE_INTERRUPTED))?;
        let (now, certain) = self.observe(&mut state)?;
        let directory = self.0.directory.join(lease.as_str());
        files::directory(&directory)?;
        let settlement = entry::Settlement {
            lease_id: lease.to_string(),
            attempt_id: attempt.to_string(),
            accounting: report.accounting().clone(),
        };
        let accounting = serde_json::to_vec(&settlement).map_err(error::encode)?;
        let target = directory.join(files::ACCOUNTING);
        if target.exists() {
            if files::read(&target, 8192)? != accounting {
                return Err(error::config(
                    "a report identity was reused with changed accounting",
                ));
            }
            return Ok(());
        }
        files::write(&target, &accounting)?;
        if let Report::Terminal { answer, .. } = report
            && certain
        {
            let metadata = entry::Metadata {
                lease_id: lease.to_string(),
                attempt_id: attempt.to_string(),
                report_id: settlement.accounting.report_id,
                generation: settlement.accounting.deletion_generation,
                first_written_at: now,
                expires_at: now.saturating_add(PRIVATE_TTL_MS),
            };
            let answer =
                zeroize::Zeroizing::new(serde_json::to_string(&answer).map_err(error::encode)?);
            cipher::seal(&directory, metadata, &answer)?;
            state.deadlines.insert(
                directory,
                Instant::now() + std::time::Duration::from_millis(PRIVATE_TTL_MS.cast_unsigned()),
            );
        }
        Ok(())
    }

    fn observe(&self, state: &mut State) -> Result<(i64, bool)> {
        let now = self.0.clock.now().as_millis();
        let certain = !state.uncertain && now >= state.high_water;
        state.high_water = state.high_water.max(now);
        files::write(
            &self.0.directory.join(files::CLOCK),
            &serde_json::to_vec(&state.high_water).map_err(error::encode)?,
        )?;
        Ok((now, certain))
    }

    /// Cleans every entry and answers the ones holding accounting to deliver.
    ///
    /// A failure inside one entry holds that entry alone: its content stays on
    /// disk and is never loaded, the failure is logged, and the pass moves on
    /// to the rest. Only a failure of the spool itself fails the pass.
    fn scan(&self) -> Result<Scanned> {
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_poisoned| error::config(STATE_INTERRUPTED))?;
        let (now, certain) = self.observe(&mut state)?;
        let (mut pending, mut held) = (Vec::new(), Vec::new());
        for item in fs::read_dir(&self.0.directory)? {
            let item = item?;
            if item.file_name() == files::CLOCK {
                continue;
            }
            let path = item.path();
            match visit(&item, &path, &mut state, now, certain) {
                Ok(true) => pending.push(path),
                Ok(false) => {}
                Err(failure) => held.push((path, failure)),
            }
        }
        let held_paths = held.iter().map(|(path, _)| path);
        let present: std::collections::HashSet<_> = pending.iter().chain(held_paths).collect();
        state.deadlines.retain(|path, _| present.contains(path));
        state.uncertain = false;
        Ok(Scanned { pending, held })
    }

    async fn load(&self, path: PathBuf) -> Result<(String, Report)> {
        let spool = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut state = spool
                .0
                .state
                .lock()
                .map_err(|_poisoned| error::config(STATE_INTERRUPTED))?;
            let (now, certain) = spool.observe(&mut state)?;
            entry::load(&path, &mut state, now, certain)
        })
        .await?
    }
}

/// One pass over the spool: the entries to deliver, and the entries held
/// because their cleanup failed, each with its failure. The caller logs the
/// held ones on its own thread, where its subscriber is.
struct Scanned {
    pending: Vec<PathBuf>,
    held: Vec<(PathBuf, crate::Error)>,
}

/// Cleans one spool item: answers whether it is an entry still holding
/// accounting to deliver, after removing anything else it finds.
fn visit(
    item: &fs::DirEntry,
    path: &std::path::Path,
    state: &mut State,
    now: i64,
    certain: bool,
) -> Result<bool> {
    let entry = item.file_type()?.is_dir()
        && item
            .file_name()
            .to_str()
            .is_some_and(|id| Uuid7::parse(id).is_ok());
    if entry {
        entry::clean(path, state, now, certain)?;
        if path.join(files::ACCOUNTING).exists() {
            return Ok(true);
        }
    }
    files::remove(path)?;
    Ok(false)
}

#[cfg(test)]
mod tests;
