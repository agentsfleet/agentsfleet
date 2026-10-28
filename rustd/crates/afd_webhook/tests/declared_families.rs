//! Recipe selection and byte preservation, independent of the HTTP layer.

use afd_crypto::mac::HmacSha256Tag;
use afd_crypto::secret::SecretBytes;
use afd_webhook::{Family, Proof, Refusal, Verdict};
use base64::{Engine as _, engine::general_purpose::STANDARD};

const NOW: i64 = 1_700_000_000;

#[test]
fn every_family_is_declarable_and_listed() {
    let expected = [
        Family::BodyHexSha256,
        Family::BodyHex,
        Family::BodyBase64Sha256,
        Family::SlackV0,
        Family::SvixV1,
    ];
    assert_eq!(Family::ALL, expected);
    let tokens: std::collections::BTreeSet<_> = Family::ALL.iter().map(|f| f.token()).collect();
    assert_eq!(tokens.len(), expected.len());
    assert_eq!(Family::for_source("acme"), None);
    assert_eq!(Family::for_source("github"), Some(Family::BodyHexSha256));
}

#[test]
fn a_base64_family_verifies_the_raw_body() {
    let secret = SecretBytes::new(b"shared fixture key".to_vec());
    let body = &[0xff, 0x00, 0x80, b'\n'];
    let tag = HmacSha256Tag::compute_peppered(&secret, &[body]);
    let signature = STANDARD.encode(tag.as_bytes());
    let proof = Proof {
        signature: Some(&signature),
        timestamp: None,
        id: None,
        body,
    };
    assert_eq!(
        Family::BodyBase64Sha256.verify_at(&secret, proof, NOW),
        Verdict::Verified
    );
    for wrong in [
        tag.to_hex(),
        format!("sha256={signature}"),
        String::new(),
        signature.chars().take(10).collect(),
    ] {
        assert_eq!(
            Family::BodyBase64Sha256.verify_at(
                &secret,
                Proof {
                    signature: Some(&wrong),
                    ..proof
                },
                NOW
            ),
            Verdict::Refused(Refusal::Signature)
        );
    }
    assert_each_byte_is_signed(&secret, proof, *body);
    assert_eq!(
        Family::BodyBase64Sha256.verify_at(&SecretBytes::new(Vec::new()), proof, NOW),
        Verdict::Refused(Refusal::Unconfigured)
    );
    assert_eq!(
        Family::BodyBase64Sha256.verify_at(
            &secret,
            Proof {
                signature: None,
                ..proof
            },
            NOW
        ),
        Verdict::Refused(Refusal::Signature)
    );
}

fn assert_each_byte_is_signed(secret: &SecretBytes, proof: Proof<'_>, body: [u8; 4]) {
    for index in 0..body.len() {
        let mut changed = body;
        for (offset, byte) in changed.iter_mut().enumerate() {
            *byte ^= u8::from(offset == index);
        }
        assert_eq!(
            Family::BodyBase64Sha256.verify_at(
                secret,
                Proof {
                    body: &changed,
                    ..proof
                },
                NOW
            ),
            Verdict::Refused(Refusal::Signature)
        );
    }
}

#[test]
fn a_family_never_borrows_another_familys_proof() {
    let secret = SecretBytes::new(b"shared fixture key".to_vec());
    let body = b"{}";
    let tag = HmacSha256Tag::compute_peppered(&secret, &[body]);
    let signature = format!("sha256={}", tag.to_hex());
    let proof = Proof {
        signature: Some(&signature),
        timestamp: None,
        id: None,
        body,
    };
    assert_eq!(
        Family::BodyHexSha256.verify_at(&secret, proof, NOW),
        Verdict::Verified
    );
    for family in [
        Family::BodyHex,
        Family::BodyBase64Sha256,
        Family::SlackV0,
        Family::SvixV1,
    ] {
        assert_ne!(family.verify_at(&secret, proof, NOW), Verdict::Verified);
    }
}
