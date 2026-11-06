//! Checkpoint identity custody authenticates every owner and version dimension.
#![expect(clippy::unwrap_used, reason = "test fixture failures must be visible")]

use afd_core::id::Uuid7;
use afd_crypto::aad::Aad;
use afd_crypto::checkpoint_digest::history_root;
use afd_crypto::envelope::Sealer;
use afd_crypto::secret::Kek;

#[test]
fn checkpoint_identity_cannot_be_rebound_to_another_owner_or_format() {
    let owner = Uuid7::parse("019f01d0-0000-7000-8000-000000000001").unwrap();
    let identity = Uuid7::parse("019f01d0-0000-7000-8000-000000000002").unwrap();
    let other = Uuid7::parse("019f01d0-0000-7000-8000-000000000003").unwrap();
    let key = Kek::from_hex(&hex::encode([0x71; 32])).unwrap();
    let scope = Aad::checkpoint_identity(&owner, &identity, "github:repo:123", 1, 2);
    let sealed = Sealer::new().seal(&key, &scope, b"identity-key").unwrap();
    assert_eq!(sealed.open(&key, &scope).unwrap().expose(), b"identity-key");
    for wrong_scope in [
        Aad::checkpoint_identity(&other, &identity, "github:repo:123", 1, 2),
        Aad::checkpoint_identity(&owner, &other, "github:repo:123", 1, 2),
        Aad::checkpoint_identity(&owner, &identity, "github:repo:124", 1, 2),
        Aad::checkpoint_identity(&owner, &identity, "github:repo:123", 2, 2),
        Aad::checkpoint_identity(&owner, &identity, "github:repo:123", 1, 3),
        Aad::effect_recovery(&owner, &identity),
        Aad::new(owner.as_str(), identity.as_str()),
    ] {
        let failure = sealed.open(&key, &wrong_scope).unwrap_err();
        assert!(failure.is_open_failed());
        assert_eq!(
            failure.code(),
            afd_core::error_code::INTERNAL_OPERATION_FAILED
        );
    }
}

#[test]
fn conversation_identity_detects_reordering_truncation_and_boundary_changes() {
    let original = history_root([("chunk-a", "digest-a", 10), ("chunk-b", "digest-b", 20)]);
    for changed in [
        history_root([("chunk-b", "digest-b", 20), ("chunk-a", "digest-a", 10)]),
        history_root([("chunk-a", "digest-a", 10)]),
        history_root([("chunk-a", "digest-a", 11), ("chunk-b", "digest-b", 20)]),
        history_root([("chunk-a", "digest-z", 10), ("chunk-b", "digest-b", 20)]),
        history_root([("chunk-z", "digest-a", 10), ("chunk-b", "digest-b", 20)]),
    ] {
        assert_ne!(original, changed);
    }
    assert_ne!(
        history_root([("ab", "c", 1)]),
        history_root([("a", "bc", 1)])
    );
}
