//! Usage and pricing are independent of semantic answer acceptance.

use af_wire::{
    decision::DecisionUsage,
    task::DecisionReceipt,
    usage::{DECISION_SEQ_BASE, UsageLine},
};
use afd_billing::{
    Nanos, Posture,
    biller::{Account, Attribution, Settlement, WorkKey},
};

use super::row::Call;
use crate::{Result, error::sequence_corrupt, task::TaskRefusal};

pub(super) enum Plan {
    Replay,
    Unknown {
        usage: Option<DecisionUsage>,
        canonical: bool,
    },
    Known {
        usage: DecisionUsage,
        charged: Nanos,
        settlement: Box<Settlement>,
    },
    NotSent,
}

pub(super) fn classify(call: &Call, receipt: &DecisionReceipt, digest: &str) -> Result<Plan> {
    if call.canonical_receipt_digest.as_deref() == Some(digest) && call.accounted {
        return Ok(Plan::Replay);
    }
    if matches!(receipt, DecisionReceipt::Unknown { .. })
        && call.failure_receipt_digest.as_deref() == Some(digest)
    {
        return Ok(Plan::Replay);
    }
    if call.accounted
        || call
            .canonical_receipt_digest
            .as_ref()
            .is_some_and(|saved| saved != digest)
    {
        return Err(TaskRefusal::ReplayConflict.into());
    }
    if !call.issued {
        return Err(TaskRefusal::InvalidTarget.into());
    }
    match receipt {
        DecisionReceipt::Provider { result } => match result
            .usage
            .and_then(|usage| usage.uncached_input().map(|uncached| (usage, uncached)))
        {
            Some((usage, uncached)) => known(call, usage, uncached, digest),
            None => unknown(call, digest),
        },
        DecisionReceipt::Unknown { .. } => unknown(call, digest),
        DecisionReceipt::NotSent { .. }
            if call.failure_receipt_digest.is_none() && call.known_usage.is_none() =>
        {
            Ok(Plan::NotSent)
        }
        DecisionReceipt::NotSent { .. } => Err(TaskRefusal::ReplayConflict.into()),
    }
}

fn unknown(call: &Call, digest: &str) -> Result<Plan> {
    match call.failure_receipt_digest.as_deref() {
        Some(saved) if saved == digest => Ok(Plan::Replay),
        Some(_) => Err(TaskRefusal::ReplayConflict.into()),
        None => Ok(Plan::Unknown {
            usage: None,
            canonical: false,
        }),
    }
}

fn known(call: &Call, usage: DecisionUsage, uncached: u32, digest: &str) -> Result<Plan> {
    let seq = u32::try_from(call.usage_sequence)
        .ok()
        .filter(|seq| *seq >= DECISION_SEQ_BASE)
        .ok_or_else(sequence_corrupt)?;
    let line = UsageLine {
        seq,
        at: call.price_snapshot.0.at,
        input_tokens: uncached,
        cached_input_tokens: usage.cached_input_tokens,
        cache_write_tokens: usage.cache_write_tokens,
        cache_write_5m_tokens: 0,
        cache_write_1h_tokens: 0,
        output_tokens: usage.output_tokens,
        runtime_ms: 0,
    };
    let rated = match call.price_snapshot.0.rate(&line) {
        Ok(rated) if rated.unpriced_tokens == 0 => rated,
        Ok(_) => return Ok(unpriced(call, usage, digest)),
        Err(fault) => {
            tracing::warn!(event = "dt_decision_price_unavailable", decision_call_id = call.id.as_str(),
                error_code = %fault.code(), reason = "charge_unrepresentable");
            return Ok(unpriced(call, usage, digest));
        }
    };
    let charged = rated.charged;
    let settlement = Settlement {
        work: WorkKey::FleetLease(Call::id(&call.original_lease_id, "original_lease_id")?),
        payer: Account {
            tenant: Call::id(&call.tenant_id, "tenant_id")?,
            workspace: Call::id(&call.workspace_id, "workspace_id")?,
        },
        attribution: Attribution::Fleet {
            fleet: Call::id(&call.fleet_id, "fleet_id")?,
            fleet_name: call.fleet_name.clone(),
            event_id: call.source_admission_id.clone(),
        },
        provider: call.provider.clone(),
        model: call.profile_snapshot.0.profile.model.clone(),
        posture: Posture::Platform,
        lines: vec![rated],
    };
    Ok(Plan::Known {
        usage,
        charged,
        settlement: Box::new(settlement),
    })
}

fn unpriced(call: &Call, usage: DecisionUsage, digest: &str) -> Plan {
    if call.canonical_receipt_digest.as_deref() == Some(digest)
        && call
            .known_usage
            .as_ref()
            .is_some_and(|saved| saved.0 == usage)
    {
        Plan::Replay
    } else {
        Plan::Unknown {
            usage: Some(usage),
            canonical: true,
        }
    }
}
