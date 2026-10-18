use std::cell::Cell;

use afd_dragonfly::EventId;
use jiff::Timestamp;

use super::super::Pending;
use super::drain;

/// The instant every fixture row carries; the pass never reads it.
const ADMITTED_AT: Timestamp = Timestamp::constant(1_760_000_000, 0);

fn rows(count: usize) -> Vec<Pending> {
    (0..count)
        .map(|index| Pending {
            id: format!("row-{index}"),
            fleet_id: "fleet".to_owned(),
            workspace_id: "workspace".to_owned(),
            actor: "actor".to_owned(),
            event_type: "chat".to_owned(),
            request_json: "{}".to_owned(),
            event_created_at: ADMITTED_AT,
        })
        .collect()
}

fn ids(rows: &[Pending]) -> Vec<&str> {
    rows.iter().map(|row| row.id.as_str()).collect()
}

/// One pass over a fake queue, and what it must come to.
struct Case {
    /// Rows the pass claimed.
    claimed: usize,
    /// Appends the queue takes before it refuses.
    takes: usize,
    appended: &'static [&'static str],
    unreached: &'static [&'static str],
    /// Appends the queue is asked for.
    calls: usize,
}

/// A fake queue takes the first `takes` appends and refuses the rest; it
/// counts every call, so a pass that kept asking after a refusal fails.
#[tokio::test]
async fn a_pass_stops_at_the_first_refusal_and_keeps_every_unreached_row() {
    let cases = [
        Case {
            claimed: 3,
            takes: 3,
            appended: &["row-0", "row-1", "row-2"],
            unreached: &[],
            calls: 3,
        },
        Case {
            claimed: 5,
            takes: 2,
            appended: &["row-0", "row-1"],
            unreached: &["row-2", "row-3", "row-4"],
            calls: 3,
        },
        Case {
            claimed: 2,
            takes: 0,
            appended: &[],
            unreached: &["row-0", "row-1"],
            calls: 1,
        },
        Case {
            claimed: 0,
            takes: 0,
            appended: &[],
            unreached: &[],
            calls: 0,
        },
    ];
    for Case {
        claimed,
        takes,
        appended,
        unreached,
        calls,
    } in cases
    {
        let batch = rows(claimed);
        let asked = Cell::new(0_usize);
        let drained = drain(&batch, |_pending| {
            let call = asked.get();
            asked.set(call + 1);
            let receipt = (call < takes).then(|| EventId::of(&format!("{call}-0")));
            async move { receipt }
        })
        .await;

        let got: Vec<&str> = drained
            .appended
            .iter()
            .map(|(pending, _)| pending.id.as_str())
            .collect();
        assert_eq!(got, appended, "{claimed} claimed, {takes} taken: appended");
        assert_eq!(
            ids(drained.unreached),
            unreached,
            "{claimed} claimed, {takes} taken: unreached"
        );
        assert_eq!(
            asked.get(),
            calls,
            "{claimed} claimed, {takes} taken: the queue is never asked past a refusal"
        );
    }
}

#[tokio::test]
async fn each_appended_row_keeps_the_receipt_its_own_append_answered() {
    let batch = rows(2);
    let drained = drain(&batch, |pending| {
        let receipt = EventId::of(&format!("{}-receipt", pending.id));
        async move { Some(receipt) }
    })
    .await;
    for (pending, receipt) in &drained.appended {
        assert_eq!(receipt.as_str(), format!("{}-receipt", pending.id));
    }
}
