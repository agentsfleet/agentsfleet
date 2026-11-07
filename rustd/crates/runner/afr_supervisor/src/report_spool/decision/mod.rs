//! Original-owner receipts survive runner replacement without retaining provider keys.

mod cipher;
#[cfg(test)]
mod tests;

use super::{ReportSpool, authoring::files};
use crate::{Result, client::ControlPlane, error};
use af_core::id::Uuid7;
use af_wire::task::{
    DecisionReceipt, DecisionReceiptInput, DecisionReceiptResult, DecisionUnavailableCause,
};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use zeroize::Zeroizing;

const DIRECTORY: &str = "decisions";
const ACTIVE: &str = "active";
const PAGE: usize = 128;
const CURSOR: &str = "after";

struct Scanned {
    pending: Vec<Result<Pending>>,
    again: bool,
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    lease_id: String,
    call_id: String,
    input: DecisionReceiptInput,
}

/// An immutable native call identity; a failed send never grants another dispatch.
pub(crate) struct Pending {
    directory: PathBuf,
    lease: Uuid7,
    call: Uuid7,
    input: DecisionReceiptInput,
    active: bool,
}

impl ReportSpool {
    /// A committed uncertainty record precedes the physical request.
    pub(crate) async fn begin_decision(
        &self,
        lease: &Uuid7,
        call: &Uuid7,
        fence: u64,
    ) -> Result<Pending> {
        let mut pending = Pending {
            directory: self.dir.join(DIRECTORY).join(call.as_str()),
            lease: lease.clone(),
            call: call.clone(),
            active: false,
            input: DecisionReceiptInput {
                fencing_token: fence,
                receipt: DecisionReceipt::Unknown {
                    cause: DecisionUnavailableCause::Transport,
                },
            },
        };
        tokio::task::spawn_blocking(move || {
            let parent = pending
                .directory
                .parent()
                .ok_or_else(|| error::config("decision spool has no parent"))?;
            files::directory(parent)?;
            // An existing identity is never overwritten into permission to send again.
            fs::create_dir(&pending.directory)?;
            // Only this successful creator owns the marker. A duplicate must
            // never release another in-flight dispatch when its value drops.
            pending.active = true;
            files::directory(&pending.directory)?;
            cipher::create(&pending.directory)?;
            files::write(&pending.directory.join(ACTIVE), b"")?;
            pending.write()?;
            Ok(pending)
        })
        .await?
    }

    /// Called once before workers start; no previous process can still publish a receipt.
    pub(crate) async fn recover_decisions(&self) -> Result<()> {
        let directory = self.dir.join(DIRECTORY);
        tokio::task::spawn_blocking(move || {
            files::directory(&directory)?;
            for item in fs::read_dir(&directory)? {
                let item = item?;
                if !item.file_type()?.is_dir() {
                    continue;
                }
                if item.path().join(cipher::CONTENT).is_file() {
                    files::remove(&item.path().join(ACTIVE))?;
                }
            }
            Ok(())
        })
        .await?
    }

    /// One bounded page on the existing report drainer, with no provider redispatch.
    pub(crate) async fn drain_decisions(&self, plane: &ControlPlane) -> Result<bool> {
        let directory = self.dir.join(DIRECTORY);
        let scanned = tokio::task::spawn_blocking(move || scan(&directory)).await??;
        let mut retained = scanned.again;
        for pending in scanned.pending {
            let delivered = match pending {
                Ok(pending) => pending.deliver(plane).await,
                Err(failure) => Err(failure),
            };
            match delivered {
                Ok(_) => {}
                Err(failure) => {
                    if failure.is_unauthorized() {
                        return Err(failure);
                    }
                    retained = true;
                    tracing::warn!(event = "decision_receipt_waiting", error_code = %failure.code());
                }
            }
        }
        Ok(retained)
    }
}

impl Pending {
    fn write(&self) -> Result<()> {
        let receipt = Receipt {
            lease_id: self.lease.to_string(),
            call_id: self.call.to_string(),
            input: self.input.clone(),
        };
        let bytes = Zeroizing::new(serde_json::to_vec(&receipt).map_err(error::encode)?);
        cipher::write(&self.directory, self.call.as_str(), &bytes)
    }

    /// Only completed receipts enter the online drain. A crash leaves the initial uncertainty.
    pub(crate) async fn finish(mut self, receipt: DecisionReceipt) -> Result<(Self, Result<()>)> {
        self.input = garde::Unvalidated::new(DecisionReceiptInput {
            fencing_token: self.input.fencing_token,
            receipt,
        })
        .validate()?
        .into_inner();
        tokio::task::spawn_blocking(move || {
            let saved = self
                .write()
                .and_then(|()| files::remove(&self.directory.join(ACTIVE)));
            if saved.is_ok() {
                self.active = false;
            }
            (self, saved)
        })
        .await
        .map_err(crate::Error::from)
    }

    pub(crate) async fn deliver(&self, plane: &ControlPlane) -> Result<DecisionReceiptResult> {
        let result = plane
            .task_decision_record(&self.lease, &self.call, &self.input)
            .await?;
        if result.call_id != self.call.as_str() {
            return Err(error::config(
                "decision receipt acknowledgement names another call",
            ));
        }
        let directory = self.directory.clone();
        tokio::task::spawn_blocking(move || files::remove(&directory)).await??;
        Ok(result)
    }
}

fn load(directory: &std::path::Path, call: &str) -> Result<Receipt> {
    let bytes = cipher::read(directory, call)?;
    let receipt: Receipt = serde_json::from_slice(bytes.expose())
        .map_err(error::malformed(crate::client::Verb::TaskDecisionRecord))?;
    if receipt.call_id != call {
        return Err(error::config(
            "decision receipt identity does not match its path",
        ));
    }
    Ok(receipt)
}

#[derive(Serialize, Deserialize)]
struct Cursor {
    after: String,
    through: String,
}

fn scan(directory: &std::path::Path) -> Result<Scanned> {
    let mut candidates = std::collections::BTreeMap::new();
    if !directory.try_exists()? {
        return Ok(Scanned {
            pending: vec![],
            again: false,
        });
    }
    let cursor_path = directory.join(CURSOR);
    let cursor: Option<Cursor> = if cursor_path.try_exists()? {
        Some(
            serde_json::from_slice(&files::read(&cursor_path, 256)?)
                .map_err(error::malformed(crate::client::Verb::TaskDecisionRecord))?,
        )
    } else {
        None
    };
    let mut through = cursor.as_ref().map(|cursor| cursor.through.clone());
    for item in fs::read_dir(directory)? {
        let item = item?;
        if !item.file_type()?.is_dir() || item.path().join(ACTIVE).exists() {
            continue;
        }
        let Some(name) = item.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if Uuid7::parse(&name).is_err() {
            continue;
        }
        if cursor
            .as_ref()
            .is_some_and(|cursor| name <= cursor.after || name > cursor.through)
        {
            continue;
        }
        if through.as_ref().is_none_or(|last| last < &name) {
            through = Some(name.clone());
        }
        candidates.insert(name, item.path());
        if candidates.len() > PAGE {
            candidates.pop_last();
        }
    }
    if let Some((name, _)) = candidates.last_key_value() {
        let next = Cursor {
            after: name.clone(),
            through: through.unwrap_or_else(|| name.clone()),
        };
        files::write(
            &cursor_path,
            &serde_json::to_vec(&next).map_err(error::encode)?,
        )?;
    } else {
        files::remove(&cursor_path)?;
    }
    let again = !candidates.is_empty() || cursor.is_some();
    let pending = candidates
        .into_iter()
        .map(|(name, directory)| {
            let call = Uuid7::parse(&name)?;
            let receipt = load(&directory, &name)?;
            Ok(Pending {
                directory,
                lease: Uuid7::parse(&receipt.lease_id)?,
                call,
                input: receipt.input,
                active: false,
            })
        })
        .collect();
    Ok(Scanned { pending, again })
}

impl std::fmt::Debug for Pending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingDecision")
            .field("call", &self.call)
            .finish_non_exhaustive()
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        if self.active
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            let marker = self.directory.join(ACTIVE);
            // A cancelled future cannot publish a later result. Its durable uncertainty
            // is now safe for the drainer; process death takes the startup path.
            runtime.spawn_blocking(move || {
                if let Err(failure) = files::remove(&marker) {
                    tracing::warn!(event = "decision_receipt_release_deferred", error_code = %failure.code());
                }
            });
        }
    }
}

#[cfg(test)]
mod delivery_tests;
