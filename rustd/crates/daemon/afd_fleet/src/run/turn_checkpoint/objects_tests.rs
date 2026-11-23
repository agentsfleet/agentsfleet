//! A transfer names each object once.

use af_core::error_code;
use af_wire::run::CheckpointOperationId;

use super::objects;

// Value: protects=one authorization never signs the same object twice;
// fails_when=ids stops refusing a repeated identity; why_new=the duplicate arm
// had no case; seam=objects::ids on the identities an Authorize names.
#[test]
fn a_transfer_naming_an_object_twice_conflicts() {
    let first = CheckpointOperationId("01900000-0000-7000-8000-000000000001".to_owned());
    let second = CheckpointOperationId("01900000-0000-7000-8000-000000000002".to_owned());
    let distinct = objects::ids(&[first.clone(), second]).map(|ids| ids.len());
    assert_eq!(distinct.ok(), Some(2));
    let repeated = objects::ids(&[first.clone(), first])
        .err()
        .map(|fault| fault.code());
    assert_eq!(repeated, Some(error_code::RUN_CHECKPOINT_CONFLICT));
}
