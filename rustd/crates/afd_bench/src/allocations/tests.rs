use super::{Counting, Snapshot, installed};

/// A heap that shrank below where it started.
const SHRUNK_BYTES: u64 = 1_000;

// The lib's unit-test binary counts, so `installed` has something to find.
// Every other unit test in the crate shares this binary and allocates beside
// these, which is why the assertions below are lower bounds.
#[global_allocator]
static COUNTING: Counting = Counting;

#[test]
fn an_installed_counter_is_found() {
    assert!(installed());
}

#[test]
fn an_allocation_is_counted_and_its_bytes_held_until_freed() {
    let before = Snapshot::now();
    let held = std::hint::black_box(vec![0_u8; 4_096]);
    let during = Snapshot::now();

    assert!(during.allocations_since(before) >= 1);
    assert!(during.bytes_gained_since(before) >= 4_096 || held.is_empty());
    drop(held);
}

#[test]
fn a_shrinking_heap_reads_as_no_gain_rather_than_a_wrapped_one() {
    let earlier = Snapshot {
        allocations: 10,
        live_bytes: 9_000,
    };
    let later = Snapshot {
        allocations: 12,
        live_bytes: SHRUNK_BYTES,
    };

    assert_eq!(later.bytes_gained_since(earlier), 0);
    assert_eq!(later.allocations_since(earlier), 2);
}
