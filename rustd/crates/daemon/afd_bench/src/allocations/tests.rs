use super::{Counting, Snapshot, installed, this_thread};

/// What the counted test holds.
const HELD_BYTES: usize = 4_096;

/// A heap that shrank below where it started.
const SHRUNK_BYTES: u64 = 1_000;

// The lib's unit-test binary counts, so `installed` has something to find.
// Every other unit test in the crate shares this binary and allocates beside
// these, so a process-wide figure is a lower bound and an exact one is read
// from the test thread's own tally.
#[global_allocator]
static COUNTING: Counting = Counting;

#[test]
fn an_installed_counter_is_found() {
    assert!(installed());
}

#[test]
fn an_allocation_is_counted_and_its_bytes_held_until_freed() {
    let before = Snapshot::now();
    let (thread_allocations, thread_bytes) = this_thread::tally();
    let held = std::hint::black_box(vec![0_u8; HELD_BYTES]);
    let (allocations_during, bytes_during) = this_thread::tally();
    let during = Snapshot::now();
    drop(held);
    let (_, bytes_after) = this_thread::tally();

    // The process-wide count only rises, so other tests cannot hide this one.
    assert!(during.allocations_since(before) >= 1);
    // Bytes come from this thread's tally alone: another test freeing memory
    // between two process-wide snapshots would move them under this one.
    assert_eq!(allocations_during.wrapping_sub(thread_allocations), 1);
    assert_eq!(bytes_during.wrapping_sub(thread_bytes), HELD_BYTES);
    assert_eq!(bytes_after, thread_bytes);
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
