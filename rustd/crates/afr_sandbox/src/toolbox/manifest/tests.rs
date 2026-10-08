#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly, and a step edits \
              the JSON it just built"
)]

use rustls_pki_types::SubjectPublicKeyInfoDer;
use rustls_pki_types::pem::PemObject as _;
use serde_json::json;

use super::{MANIFEST_MAX_BYTES, Release, debian_arch, host_arch};
use crate::error::ToolboxRefusal;
use crate::toolbox::testing::{RUNNER, Signer, facts, manifest_bytes, sha256};

/// The interop fixture: a real release manifest, trimmed to three packages,
/// signed with `cosign sign-blob` under [`FIXTURE_PUBLIC_KEY`]'s private half,
/// which was then discarded, and checked with `cosign verify-blob` before it
/// was committed.
const COSIGN_MANIFEST: &[u8] = include_bytes!("../fixtures/release.json");
const COSIGN_SIGNATURE: &[u8] = include_bytes!("../fixtures/release.json.sig");
/// What that fixture names.
const COSIGN_RUNNER: &str = "0.56.0";
const COSIGN_DIGEST: &str = "377a14f4401c17acc47c6d90f621b550b9e07b49039cb7831eb48a5a55416402";
const COSIGN_LENGTH: u64 = 258_277_376;
const IMAGE: &[u8] = b"an erofs image's bytes";
/// The key the fixture was signed under: a test key, never the release key,
/// so the real image the fixture names is admitted by no host.
const FIXTURE_PUBLIC_KEY: &str = "-----BEGIN PUBLIC KEY-----
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAELVQWBgCxq2ODgOCaj/XkI/Vlvbjz
NE5hNFWfWCzZc7dlzHFBosEsGA1965zFODW/81o74kL/hvsesQ2gmbWd8A==
-----END PUBLIC KEY-----
";

/// Which check `release` fails `manifest` signed as `signature` on.
fn refusal(release: &Release, manifest: &[u8], signature: &[u8]) -> Option<ToolboxRefusal> {
    release
        .verify(manifest, signature)
        .err()
        .and_then(|refused| refused.toolbox_refusal())
}

#[test]
fn a_release_signed_by_its_key_names_its_image() {
    let signer = Signer::new();
    let bytes = manifest_bytes(&facts(IMAGE));

    let manifest = signer
        .release()
        .verify(&bytes, &signer.sign(&bytes))
        .unwrap();

    assert_eq!(manifest.sha256(), sha256(IMAGE));
    assert_eq!(manifest.length(), IMAGE.len() as u64);
}

#[test]
fn the_cosign_fixture_verifies_against_the_key_it_was_signed_under() {
    let key = SubjectPublicKeyInfoDer::from_pem_slice(FIXTURE_PUBLIC_KEY.as_bytes()).unwrap();
    let release = Release::signed_by(key.as_ref(), COSIGN_RUNNER).on("arm64");

    let manifest = release.verify(COSIGN_MANIFEST, COSIGN_SIGNATURE).unwrap();

    assert_eq!(manifest.sha256(), COSIGN_DIGEST);
    assert_eq!(manifest.length(), COSIGN_LENGTH);
}

#[test]
fn should_refuse_the_cosign_fixture_under_the_release_key() {
    let release = Release::signed_by_release(COSIGN_RUNNER)
        .unwrap()
        .on("arm64");

    assert_eq!(
        refusal(&release, COSIGN_MANIFEST, COSIGN_SIGNATURE),
        Some(ToolboxRefusal::Signature)
    );
}

#[test]
fn should_refuse_a_signature_that_is_not_the_release_keys() {
    let (signer, stranger) = (Signer::new(), Signer::new());
    let bytes = manifest_bytes(&facts(IMAGE));
    let mut tampered = bytes.clone();
    tampered[1] ^= 1;
    let oversized = vec![b' '; MANIFEST_MAX_BYTES + 1];
    let release = signer.release();

    for (manifest, signature) in [
        (bytes.clone(), stranger.sign(&bytes)),
        (tampered, signer.sign(&bytes)),
        (bytes.clone(), b"not base64 at all!".to_vec()),
        (bytes.clone(), Vec::new()),
        (oversized.clone(), signer.sign(&oversized)),
    ] {
        assert_eq!(
            refusal(&release, &manifest, &signature),
            Some(ToolboxRefusal::Signature)
        );
    }
    assert_eq!(
        refusal(
            &Release::signed_by_release(RUNNER).unwrap(),
            &bytes,
            &signer.sign(&bytes)
        ),
        Some(ToolboxRefusal::Signature),
        "a manifest the suite signed is not the release's"
    );
}

#[test]
fn should_refuse_a_signed_manifest_it_cannot_read() {
    let signer = Signer::new();
    let mut unreadable = Vec::new();
    for edit in [
        |facts: &mut serde_json::Value| facts["runner_versions"] = json!([]),
        |facts: &mut serde_json::Value| facts["length"] = json!(0),
        |facts: &mut serde_json::Value| facts["arch"] = json!(""),
        |facts: &mut serde_json::Value| facts["sha256"] = json!(7),
        // The digest names files under the toolbox directory before the
        // image is hashed, so it is held to sixty-four lowercase hex digits.
        |facts: &mut serde_json::Value| facts["sha256"] = json!("../../etc/passwd"),
        |facts: &mut serde_json::Value| facts["sha256"] = json!("a".repeat(63)),
        |facts: &mut serde_json::Value| facts["sha256"] = json!("A".repeat(64)),
        |facts: &mut serde_json::Value| facts["sha256"] = json!("g".repeat(64)),
        |facts: &mut serde_json::Value| facts["erofs_features"] = json!(["x".repeat(200)]),
    ] {
        let mut changed = facts(IMAGE);
        edit(&mut changed);
        unreadable.push(manifest_bytes(&changed));
    }
    unreadable.push(b"{\"arch\": ".to_vec());
    unreadable.push(manifest_bytes(&json!({"arch": "arm64"})));

    for manifest in unreadable {
        assert_eq!(
            refusal(&signer.release(), &manifest, &signer.sign(&manifest)),
            Some(ToolboxRefusal::Manifest),
            "{}",
            String::from_utf8_lossy(&manifest)
        );
    }
}

#[test]
fn should_refuse_a_release_for_another_host_runner_or_kernel() {
    let signer = Signer::new();
    let bytes = manifest_bytes(&facts(IMAGE));
    let mut later = facts(IMAGE);
    later["runner_versions"] = json!(["10.0.0"]);
    let mut packed = facts(IMAGE);
    packed["erofs_features"] = json!(["sb_csum", "ztailpacking"]);
    let (later, packed) = (manifest_bytes(&later), manifest_bytes(&packed));

    let cases = [
        (
            signer.release().on("amd64"),
            &bytes,
            ToolboxRefusal::Architecture,
        ),
        (signer.release(), &later, ToolboxRefusal::RunnerVersion),
        (signer.release(), &packed, ToolboxRefusal::Features),
    ];

    for (release, manifest, expected) in cases {
        assert_eq!(
            refusal(&release, manifest, &signer.sign(manifest)),
            Some(expected)
        );
    }
}

#[test]
fn an_image_is_held_to_the_manifests_length_then_its_digest() {
    let manifest = Signer::new().manifest(IMAGE);
    let length = IMAGE.len() as u64;
    let refused = |checked: crate::Result<()>| checked.err().and_then(|e| e.toolbox_refusal());

    manifest.check_length(length).unwrap();
    manifest.check_digest(&sha256(IMAGE)).unwrap();
    for wrong in [length - 1, length + 1] {
        assert_eq!(
            refused(manifest.check_length(wrong)),
            Some(ToolboxRefusal::Length)
        );
    }
    assert_eq!(
        refused(manifest.check_digest(&sha256(b"another image"))),
        Some(ToolboxRefusal::Digest)
    );
}

#[test]
fn each_refusal_is_spelled_apart_from_every_other() {
    let all = [
        ToolboxRefusal::Signature,
        ToolboxRefusal::Manifest,
        ToolboxRefusal::Architecture,
        ToolboxRefusal::RunnerVersion,
        ToolboxRefusal::Features,
        ToolboxRefusal::Length,
        ToolboxRefusal::Digest,
        ToolboxRefusal::NotAFile,
    ];
    let mut spelled: Vec<&str> = all.iter().map(|refusal| refusal.as_str()).collect();

    spelled.sort_unstable();
    spelled.dedup();

    assert_eq!(spelled.len(), all.len());
    assert_eq!(ToolboxRefusal::Digest.to_string(), "digest_mismatch");
}

/// The size cap sits at exactly a mebibyte for the manifest and its
/// signature alike: one at the cap is read, one byte past it is not.
#[test]
fn should_read_up_to_the_size_cap_and_refuse_past_it() {
    let signer = Signer::new();
    let at_cap = vec![b' '; MANIFEST_MAX_BYTES];
    let detail = |manifest: &[u8], signature: &[u8]| {
        signer
            .release()
            .verify(manifest, signature)
            .err()
            .map(|refused| refused.to_string())
            .unwrap_or_default()
    };

    assert_eq!(MANIFEST_MAX_BYTES, 1_048_576);
    assert_eq!(
        refusal(&signer.release(), &at_cap, &signer.sign(&at_cap)),
        Some(ToolboxRefusal::Manifest),
        "a manifest at the cap is read, and is not JSON"
    );
    let bytes = manifest_bytes(&facts(IMAGE));
    let mut padded = signer.sign(&bytes);
    padded.resize(MANIFEST_MAX_BYTES, b'\n');
    signer.release().verify(&bytes, &padded).unwrap();
    padded.push(b'\n');
    assert!(
        detail(&bytes, &padded).contains("too large"),
        "{}",
        detail(&bytes, &padded)
    );
}

/// A host names its architecture as Debian does, which is how a release
/// records it.
#[test]
fn the_host_is_named_as_debian_names_it() {
    // pin test: literal is the contract — Debian's name, not Rust's.
    #[cfg(target_arch = "aarch64")]
    assert_eq!(host_arch(), "arm64");
    // pin test: literal is the contract — Debian's name, not Rust's.
    #[cfg(target_arch = "x86_64")]
    assert_eq!(host_arch(), "amd64");
}

/// A manifest that will not parse is refused with the parser's own reason
/// kept as the cause, not flattened into the message.
#[test]
fn an_unreadable_manifest_keeps_the_parsers_reason() {
    let signer = Signer::new();
    let truncated = b"{\"arch\": ".to_vec();

    let refused = signer
        .release()
        .verify(&truncated, &signer.sign(&truncated))
        .unwrap_err();

    assert_eq!(refused.toolbox_refusal(), Some(ToolboxRefusal::Manifest));
    let cause = std::error::Error::source(&refused)
        .map(ToString::to_string)
        .unwrap_or_default();
    assert!(cause.contains("EOF while parsing"), "{cause}");
    assert!(!refused.to_string().contains("EOF"), "{refused}");
}

/// Debian renames the two architectures the toolbox is built for and leaves
/// any other as Rust names it; this host's name is one of the renamed pair.
#[test]
fn an_architecture_is_named_as_debian_names_it() {
    assert_eq!(debian_arch("x86_64"), "amd64");
    assert_eq!(debian_arch("aarch64"), "arm64");
    assert_eq!(debian_arch("riscv64"), "riscv64");
    assert!(matches!(host_arch(), "amd64" | "arm64"));
}
