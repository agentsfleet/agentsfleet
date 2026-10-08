#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_wire::tool_detail::{DETAIL_POST_MAX_BYTES, ToolCallRecord, ToolCallRecordsRequest};

use super::bodies;

/// A record's output, sized so four fit one post and a fifth does not.
const RECORD_OUTPUT_BYTES: usize = 60 * 1024;

fn record(number: u64, output_bytes: usize) -> ToolCallRecord<'static> {
    ToolCallRecord {
        call_number: number,
        arguments: serde_json::Map::new(),
        truncated_arguments: false,
        output: "x".repeat(output_bytes).into(),
        output_line_count: 1,
        truncated: false,
    }
}

/// The call numbers each body carries, after checking it decodes and fits.
fn numbers(bodies: &[bytes::Bytes]) -> Vec<Vec<u64>> {
    bodies
        .iter()
        .map(|body| {
            assert!(body.len() <= DETAIL_POST_MAX_BYTES, "{} bytes", body.len());
            let request: ToolCallRecordsRequest<'_> = serde_json::from_slice(body).unwrap();
            assert_eq!(request.fencing_token, 9);
            request
                .calls
                .iter()
                .map(|raw| raw.narrow().unwrap().call_number)
                .collect()
        })
        .collect()
}

#[test]
fn should_post_small_records_in_one_body() {
    let records = [record(1, 10), record(2, 10), record(3, 10)];

    assert_eq!(numbers(&bodies(9, &records).unwrap()), [vec![1, 2, 3]]);
}

#[test]
fn should_split_records_across_bodies_at_the_post_bound_in_order() {
    let big = RECORD_OUTPUT_BYTES;
    let records: Vec<_> = (1..=6).map(|number| record(number, big)).collect();

    let posted = numbers(&bodies(9, &records).unwrap());

    assert_eq!(posted, [vec![1, 2, 3, 4], vec![5, 6]]);
}

#[test]
fn should_post_nothing_for_no_records() {
    let none_written = bodies(9, &[]).unwrap();
    assert!(none_written.is_empty(), "{none_written:?}");
    assert_eq!(numbers(&bodies(9, &[record(1, 0)]).unwrap())[0], [1]);
}

#[test]
fn should_fill_a_body_to_exactly_the_post_bound_and_split_one_byte_past_it() {
    let envelope = bodies(9, &[record(1, 0)]).unwrap()[0].len() - encoded(&record(1, 0));
    let full: Vec<_> = (1..=4)
        .map(|number| record(number, RECORD_OUTPUT_BYTES))
        .collect();
    let taken: usize = full.iter().map(|full| encoded(full) + ",".len()).sum();
    let filling = DETAIL_POST_MAX_BYTES - envelope - taken - encoded(&record(5, 0));
    let with_last = |output_bytes| {
        let mut records = full.clone();
        records.push(record(5, output_bytes));
        bodies(9, &records).unwrap()
    };

    let exact = with_last(filling);
    let over = with_last(filling + 1);

    assert_eq!(numbers(&exact), [vec![1, 2, 3, 4, 5]]);
    assert_eq!(exact[0].len(), DETAIL_POST_MAX_BYTES);
    assert_eq!(numbers(&over), [vec![1, 2, 3, 4], vec![5]]);
}

/// How many bytes `record` encodes to.
fn encoded(record: &ToolCallRecord<'_>) -> usize {
    serde_json::to_string(record).unwrap().len()
}
