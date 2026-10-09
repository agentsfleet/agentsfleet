#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::fs;
use std::path::Path;

use super::{MANIFEST_SUFFIX, SIGNATURE_SUFFIX};
use crate::error::ToolboxRefusal;
use crate::toolbox::testing::{Signer, facts, manifest_bytes, open, sha256};
use crate::toolbox::{TOOLBOX_PREFIX, image_name};

/// Stages image `n` under `incoming` as a deploy does: the image under its
/// digest, the manifest beside it, and `signer`'s signature over the manifest.
/// Returns the image's digest.
fn stage(signer: &Signer, incoming: &Path, n: u8) -> String {
    let image = vec![n; 64];
    let digest = sha256(&image);
    let manifest = manifest_bytes(&facts(&image));
    let manifest_path = incoming.join(format!("{TOOLBOX_PREFIX}{digest}{MANIFEST_SUFFIX}"));
    fs::create_dir_all(incoming).unwrap();
    fs::write(incoming.join(image_name(&digest)), &image).unwrap();
    fs::write(&manifest_path, &manifest).unwrap();
    let mut signature = manifest_path.into_os_string();
    signature.push(SIGNATURE_SUFFIX);
    fs::write(signature, signer.sign(&manifest)).unwrap();
    digest
}

/// The staged release is verified, staged into the host's images and mounted.
#[test]
fn test_the_staged_release_is_admitted() {
    let dir = tempfile::tempdir().unwrap();
    let signer = Signer::new();
    let incoming = dir.path().join("incoming");
    let digest = stage(&signer, &incoming, 1);
    let (toolboxes, recorder) = open(dir.path());

    let toolbox = toolboxes
        .admit_incoming(&signer.release(), &incoming)
        .unwrap();

    assert_eq!(toolbox.digest(), digest);
    assert_eq!(*recorder.mounted.lock().unwrap(), vec![digest]);
}

/// An empty directory and one with two releases are each refused as unstaged,
/// and nothing is mounted: a host admits exactly the release a deploy brought.
#[test]
fn test_anything_but_one_staged_release_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let signer = Signer::new();
    let empty = dir.path().join("empty");
    fs::create_dir_all(&empty).unwrap();
    let two = dir.path().join("two");
    stage(&signer, &two, 1);
    stage(&signer, &two, 2);
    let (toolboxes, recorder) = open(dir.path());

    for incoming in [&empty, &two] {
        let refused = toolboxes
            .admit_incoming(&signer.release(), incoming)
            .unwrap_err();

        assert_eq!(
            refused.toolbox_refusal(),
            Some(ToolboxRefusal::Unstaged),
            "{refused}"
        );
    }
    assert!(recorder.mounted.lock().unwrap().is_empty());
}

/// A release another key signed is refused on its signature before its image
/// is read.
#[test]
fn test_a_release_signed_by_another_key_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let incoming = dir.path().join("incoming");
    stage(&Signer::new(), &incoming, 1);
    let (toolboxes, recorder) = open(dir.path());

    let refused = toolboxes
        .admit_incoming(&Signer::new().release(), &incoming)
        .unwrap_err();

    assert_eq!(refused.toolbox_refusal(), Some(ToolboxRefusal::Signature));
    assert!(recorder.mounted.lock().unwrap().is_empty());
}

/// A manifest whose signature was never staged, or whose image was not, is
/// an input failure naming no check, and mounts nothing.
#[test]
fn test_a_release_missing_its_signature_or_image_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let signer = Signer::new();
    let (toolboxes, recorder) = open(dir.path());
    let unsigned = dir.path().join("unsigned");
    let digest = stage(&signer, &unsigned, 1);
    let mut signature = unsigned
        .join(format!("{TOOLBOX_PREFIX}{digest}{MANIFEST_SUFFIX}"))
        .into_os_string();
    signature.push(SIGNATURE_SUFFIX);
    fs::remove_file(signature).unwrap();
    let imageless = dir.path().join("imageless");
    let digest = stage(&signer, &imageless, 2);
    fs::remove_file(imageless.join(image_name(&digest))).unwrap();

    for incoming in [&unsigned, &imageless] {
        let refused = toolboxes
            .admit_incoming(&signer.release(), incoming)
            .unwrap_err();

        assert_eq!(refused.toolbox_refusal(), None, "{refused}");
    }
    assert!(recorder.mounted.lock().unwrap().is_empty());
}

/// A manifest past the most one may hold is read only that far and refused as
/// too large, never read whole.
#[test]
fn test_an_oversized_manifest_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let signer = Signer::new();
    let incoming = dir.path().join("incoming");
    let digest = stage(&signer, &incoming, 1);
    let manifest = incoming.join(format!("{TOOLBOX_PREFIX}{digest}{MANIFEST_SUFFIX}"));
    let oversized = vec![b' '; super::MANIFEST_MAX_BYTES * 2];
    fs::write(&manifest, oversized).unwrap();
    let (toolboxes, _recorder) = open(dir.path());

    let refused = toolboxes
        .admit_incoming(&signer.release(), &incoming)
        .unwrap_err();

    assert_eq!(refused.toolbox_refusal(), Some(ToolboxRefusal::Signature));
}
