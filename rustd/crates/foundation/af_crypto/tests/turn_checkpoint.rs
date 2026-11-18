//! Private turn ciphertext cannot be moved across owners, generations or formats.

use af_core::id::Uuid7;
use af_crypto::{aad::Aad, envelope::Sealer, secret::Kek};

#[test]
fn turn_history_authenticates_every_scope_and_version_dimension()
-> Result<(), Box<dyn std::error::Error>> {
    let workspace = Uuid7::parse("019f0000-0000-7000-8000-000000000001")?;
    let fleet = Uuid7::parse("019f0000-0000-7000-8000-000000000002")?;
    let event = Uuid7::parse("019f0000-0000-7000-8000-000000000003")?;
    let other = Uuid7::parse("019f0000-0000-7000-8000-000000000004")?;
    let key = Kek::from_bytes([0x63; 32]);
    let bound = Aad::turn_recovery(&workspace, &fleet, &event, 7, 1, af_crypto::KEK_VERSION);
    let sealed = Sealer::new().seal(&key, &bound, b"private root state")?;
    assert_eq!(sealed.open(&key, &bound)?.expose(), b"private root state");
    for wrong in [
        Aad::turn_recovery(&other, &fleet, &event, 7, 1, af_crypto::KEK_VERSION),
        Aad::turn_recovery(&workspace, &other, &event, 7, 1, af_crypto::KEK_VERSION),
        Aad::turn_recovery(&workspace, &fleet, &other, 7, 1, af_crypto::KEK_VERSION),
        Aad::turn_recovery(&workspace, &fleet, &event, 8, 1, af_crypto::KEK_VERSION),
        Aad::turn_recovery(&workspace, &fleet, &event, 7, 2, af_crypto::KEK_VERSION),
        Aad::turn_recovery(&workspace, &fleet, &event, 7, 1, af_crypto::KEK_VERSION + 1),
        Aad::effect_recovery(&workspace, &event),
        Aad::checkpoint_identity(
            &workspace,
            &event,
            "turn:example",
            1,
            af_crypto::KEK_VERSION,
        ),
        Aad::new(workspace.as_str(), event.as_str()),
    ] {
        assert!(sealed.open(&key, &wrong).is_err(), "{wrong:?}");
    }
    Ok(())
}
