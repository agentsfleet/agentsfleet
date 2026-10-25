//! Page complete source groups and compact before the next physical input overfills.

use serde::{Deserialize, Serialize};

use crate::{
    Checkpoint, Compaction, HistorySource, Identity, Progress, Result, Summary, TokenEstimator,
    Window, digest_groups, error::ErrorKind,
};

/// A bounded source read is independent of the lifetime conversation length.
pub const HISTORY_PAGE_SIZE: usize = 64;
const RETAIN_RECENT_GROUPS: usize = 2;

/// One complete authored exchange or assistant/tool-result group.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    /// Stable source sequence, strictly increasing in one scope.
    pub sequence: u64,
    /// Quoted source data; never elevated to trusted instructions.
    pub text: String,
}

/// Bounded authoritative source data after a sequence watermark.
#[derive(Debug)]
pub struct Page {
    /// Complete source groups, oldest first.
    pub groups: Vec<Group>,
    /// Whether another page exists after these groups.
    pub has_more: bool,
}

/// Either a sendable context or one metered summary step before retrying preflight.
#[derive(Debug)]
pub enum Plan {
    /// The original current request/pinned state surround this retained context.
    Ready {
        /// Prior untrusted notes, already verified against their exact source.
        summary: Option<Summary>,
        /// Complete retained source groups.
        groups: Vec<Group>,
        /// Output clipped to the complete input and remaining allowance.
        output_tokens: u32,
    },
    /// Spend one physical call, publish conditionally, then run preflight again.
    Compact(Compaction),
}

/// Plan without sending a model request or modifying authoritative history.
///
/// # Errors
/// Invalid source order, changed checkpoint source, cancelled work or pinned overflow.
pub async fn prepare<H, E, Stop, Observe>(
    source: &mut H,
    estimator: &mut E,
    identity: Identity,
    stored: Option<Checkpoint>,
    window: Window,
    stopped: Stop,
    mut observe: Observe,
) -> Result<Plan>
where
    H: HistorySource + ?Sized,
    E: TokenEstimator + ?Sized,
    Stop: Fn() -> bool,
    Observe: FnMut(Progress),
{
    let generation = stored
        .as_ref()
        .map_or(0, |checkpoint| checkpoint.generation);
    let checkpoint = stored.filter(|checkpoint| checkpoint.belongs_to(&identity));
    let mut read = Reading::new(checkpoint.as_ref());
    let mut estimate_at = 0;
    loop {
        if stopped() {
            return Err(crate::Error::cancelled());
        }
        let page = source.page(read.sequence, HISTORY_PAGE_SIZE).await?;
        observe(Progress::Read(page.groups.len()));
        let exhausted = !page.has_more;
        read.append(page, checkpoint.as_ref())?;
        if !read.verified || (!exhausted && read.groups.len() < estimate_at) {
            continue;
        }
        // Probe growing prefixes geometrically; always measure the complete final input.
        estimate_at = read.groups.len().max(HISTORY_PAGE_SIZE).saturating_mul(2);
        let summary = checkpoint.as_ref().map(|checkpoint| &checkpoint.summary);
        let input = estimator.continuation(summary, &read.groups)?;
        if exhausted
            && (!window.needs_compaction(input)
                || (read.groups.is_empty() && input < window.trigger_tokens()))
        {
            return Ok(Plan::Ready {
                summary: summary.cloned(),
                groups: read.groups,
                output_tokens: window.fit_output(input)?,
            });
        }
        if window.needs_compaction(input) || exhausted {
            return compact(
                read.groups,
                checkpoint,
                identity,
                generation,
                window,
                estimator,
                exhausted,
            );
        }
    }
}

struct Reading {
    sequence: u64,
    digest: String,
    verified: bool,
    groups: Vec<Group>,
}

impl Reading {
    fn new(checkpoint: Option<&Checkpoint>) -> Self {
        Self {
            sequence: 0,
            digest: String::new(),
            verified: checkpoint.is_none(),
            groups: Vec::new(),
        }
    }

    fn append(&mut self, page: Page, checkpoint: Option<&Checkpoint>) -> Result<()> {
        if page.groups.len() > HISTORY_PAGE_SIZE || (page.has_more && page.groups.is_empty()) {
            return Err(ErrorKind::Stale.into());
        }
        for group in page.groups {
            if group.sequence <= self.sequence {
                return Err(ErrorKind::Stale.into());
            }
            self.sequence = group.sequence;
            if self.verified {
                self.groups.push(group);
            } else {
                self.digest = digest_groups(&self.digest, std::slice::from_ref(&group));
                let checkpoint = checkpoint.ok_or(ErrorKind::Stale)?;
                if self.sequence >= checkpoint.covered_through {
                    checkpoint.verify_source(self.sequence, &self.digest)?;
                    self.verified = true;
                }
            }
        }
        if !page.has_more && !self.verified {
            return Err(ErrorKind::Stale.into());
        }
        Ok(())
    }
}

fn compact<E: TokenEstimator + ?Sized>(
    mut groups: Vec<Group>,
    checkpoint: Option<Checkpoint>,
    identity: Identity,
    expected_generation: u64,
    window: Window,
    estimator: &mut E,
    exhausted: bool,
) -> Result<Plan> {
    let keep = if exhausted {
        RETAIN_RECENT_GROUPS.min(groups.len().saturating_sub(1))
    } else {
        0
    };
    groups.truncate(groups.len().saturating_sub(keep));
    let mut task = Compaction {
        identity,
        expected_generation,
        prior_digest: checkpoint
            .as_ref()
            .map_or_else(String::new, |value| value.source_digest.clone()),
        prior_summary: checkpoint.map(|value| value.summary),
        groups,
        output_tokens: crate::SUMMARY_OUTPUT_TOKENS.min(window.output_tokens()),
    };
    while !task.groups.is_empty() {
        let input = estimator.compaction(&task)?;
        if input.saturating_add(u64::from(task.output_tokens)) <= window.safe_tokens() {
            return Ok(Plan::Compact(task));
        }
        // Geometric reduction bounds total encoded source work by twice the page.
        task.groups.truncate(task.groups.len() / 2);
    }
    Err(ErrorKind::Window.into())
}
