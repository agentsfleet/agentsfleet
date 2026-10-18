//! Reports use real dispatch receipts and wallet rows, independently of private text.

mod billing;
mod failure_facts;
mod replay;
mod uncertainty;
mod validation;

use super::super::{attempt::Attempt, fixture::Fixture, mint};
use super::fixture;
use afd_wire::authoring::{Accounting, Knowledge, Meter, Outcome, Report};

fn report() -> Report {
    Report::Terminal {
        accounting: Accounting {
            fence: "1".into(),
            deletion_generation: "0".into(),
            report_id: mint().to_string(),
            outcome: Outcome::Completed,
            failure_reason: None,
            usage: meter(),
        },
        answer: Some("A retained private answer.\r\n".into()),
    }
}

fn meter() -> Meter {
    Meter {
        knowledge: Knowledge::Known,
        calls: 0,
        input_tokens: Some(0),
        output_tokens: Some(0),
        runtime_ms: 1,
        provider_cost_nanos: None,
    }
}

fn accounting(request: &Report) -> Report {
    Report::Accounting {
        accounting: request.accounting().clone(),
    }
}

async fn balance(f: &Fixture) -> i64 {
    sqlx::query_scalar("SELECT balance_nanos FROM billing.wallets WHERE tenant_id=$1::uuid")
        .bind(f.tenant.as_str())
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("owned wallet")
}

async fn counts(f: &Fixture, a: &Attempt) -> (i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM authoring.attempts WHERE id=$1::uuid AND outcome IS NOT NULL), \
        (SELECT count(*) FROM billing.authoring_report_receipts WHERE attempt_id=$1::uuid), \
        (SELECT count(*) FROM authoring.answer_receipts WHERE attempt_id=$1::uuid)",
    )
    .bind(a.id.as_str())
    .fetch_one(&mut *f.connection().await)
    .await
    .expect("accounting and private receipts")
}

async fn messages(f: &Fixture, a: &Attempt) -> Vec<afd_authoring::types::Message> {
    f.drafts
        .messages(
            f.access(),
            &a.draft.id,
            afd_authoring::types::MessageQuery {
                limit: 20,
                starting_after: None,
            },
        )
        .await
        .expect("creator message recovery")
        .items
}
