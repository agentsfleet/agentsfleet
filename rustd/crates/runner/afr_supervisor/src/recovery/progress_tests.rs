use super::progress::Progress;

#[test]
fn repeated_objects_and_acknowledgements_do_not_inflate_transfer_bytes() {
    let mut progress = Progress::default();
    progress.discover("reused", 80, true);
    progress.discover("missing", 120, false);
    progress.discover("missing", 120, false);
    progress.discover("reused", 80, true);
    assert_eq!(
        (
            progress.bytes.acknowledged,
            progress.bytes.missing,
            progress.bytes.reused
        ),
        (0, 120, 80)
    );
    assert_eq!(progress.completed, 1);
    progress.acknowledge("unknown");
    progress.acknowledge("reused");
    assert_eq!(
        (
            progress.bytes.acknowledged,
            progress.bytes.missing,
            progress.bytes.reused
        ),
        (0, 120, 80)
    );
    progress.acknowledge("missing");
    progress.acknowledge("missing");
    progress.discover("missing", 120, false);
    assert_eq!(
        (
            progress.bytes.acknowledged,
            progress.bytes.missing,
            progress.bytes.reused
        ),
        (120, 0, 80)
    );
    assert_eq!(progress.completed, 2);
}
