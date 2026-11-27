//! Private turn ciphertext cannot be moved across owners, generations or formats.

use af_core::id::Uuid7;
use af_crypto::aad::Aad;
use af_crypto::checkpoint::{Aes256Gcm, ChunkCipher as _};
use af_crypto::secret::SecretBytes;
use zeroize::Zeroizing;

#[test]
fn turn_history_authenticates_every_scope_and_version_dimension()
-> Result<(), Box<dyn std::error::Error>> {
    let workspace = Uuid7::parse("019f0000-0000-7000-8000-000000000001")?;
    let fleet = Uuid7::parse("019f0000-0000-7000-8000-000000000002")?;
    let event = Uuid7::parse("019f0000-0000-7000-8000-000000000003")?;
    let other = Uuid7::parse("019f0000-0000-7000-8000-000000000004")?;
    let key = SecretBytes::new(vec![0x63; af_crypto::KEY_LEN]);
    let nonce = [0x17; af_crypto::NONCE_LEN];
    let bound = Aad::turn_recovery(&workspace, &fleet, &event, 7, 1);
    let sealed = Aes256Gcm.seal(
        &key,
        &nonce,
        &bound,
        Zeroizing::new(b"private root state".to_vec()),
    )?;
    assert_eq!(
        Aes256Gcm
            .open(&key, &nonce, &bound, sealed.clone())?
            .expose(),
        b"private root state"
    );
    for wrong in [
        Aad::turn_recovery(&other, &fleet, &event, 7, 1),
        Aad::turn_recovery(&workspace, &other, &event, 7, 1),
        Aad::turn_recovery(&workspace, &fleet, &other, 7, 1),
        Aad::turn_recovery(&workspace, &fleet, &event, 8, 1),
        Aad::turn_recovery(&workspace, &fleet, &event, 7, 2),
        Aad::new(workspace.as_str(), event.as_str()),
    ] {
        assert!(
            Aes256Gcm
                .open(&key, &nonce, &wrong, sealed.clone())
                .is_err(),
            "{wrong:?}"
        );
    }
    Ok(())
}
