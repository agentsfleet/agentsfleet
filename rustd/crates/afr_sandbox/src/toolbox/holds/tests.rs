#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::{Mounter, Toolboxes};
use crate::error::{Result, ToolboxRefusal};
use crate::toolbox::testing::{Signer, facts, manifest_bytes, sha256};
use crate::toolbox::{Manifest, Toolbox, image_name};

/// Mounts nothing; checks the image's length and digest as admission does,
/// records what it was asked to mount and unmount, and refuses unmounts while
/// told to.
#[derive(Debug, Default)]
struct Recorder {
    mounted: Mutex<Vec<String>>,
    unmounted: Mutex<Vec<String>>,
    refuse_unmounts: AtomicBool,
}

impl Mounter for Arc<Recorder> {
    fn mount(&self, manifest: &Manifest, image: &Path) -> Result<Toolbox> {
        let bytes = fs::read(image)?;
        manifest.check_length(u64::try_from(bytes.len()).unwrap())?;
        manifest.check_digest(&sha256(&bytes))?;
        let digest = manifest.sha256().to_owned();
        self.mounted.lock().unwrap().push(digest.clone());
        Ok(Toolbox::at(image.with_extension("mnt"), digest))
    }

    fn unmount(&self, toolbox: &Toolbox) -> Result<()> {
        if self.refuse_unmounts.load(Ordering::SeqCst) {
            return Err(std::io::Error::other("busy").into());
        }
        self.unmounted
            .lock()
            .unwrap()
            .push(toolbox.digest().to_owned());
        Ok(())
    }
}

/// An image, its download, and the manifest naming it.
struct Release {
    manifest: Manifest,
    source: std::path::PathBuf,
}

/// Image `n`, downloaded under `dir`.
fn release(signer: &Signer, dir: &Path, n: u8) -> Release {
    let bytes = vec![n; 64];
    let source = dir.join(format!("download-{n}"));
    fs::write(&source, &bytes).unwrap();
    Release {
        manifest: signer.manifest(&bytes),
        source,
    }
}

fn open(dir: &Path) -> (Toolboxes<Arc<Recorder>>, Arc<Recorder>) {
    let recorder = Arc::new(Recorder::default());
    let images = dir.join("images");
    fs::create_dir_all(&images).unwrap();
    (
        Toolboxes::open(images, Arc::clone(&recorder)).unwrap(),
        recorder,
    )
}

/// Dimension 8.2: a bad signature, a wrong length, the wrong architecture
/// and a half-written download are each refused, with its own code, before
/// anything is mounted.
#[test]
fn test_toolbox_admission_refusals() {
    let dir = tempfile::tempdir().unwrap();
    let (toolboxes, recorder) = open(dir.path());
    let (signer, stranger) = (Signer::new(), Signer::new());
    let image = vec![7_u8; 64];
    let bytes = manifest_bytes(&facts(&image));
    let manifest = signer.manifest(&image);
    let staged = |download: Vec<u8>| {
        let source = dir.path().join("download");
        fs::write(&source, download).unwrap();
        toolboxes.admit(&manifest, &source).err()
    };
    let mut half = image.clone();
    half.get_mut(32..).unwrap().fill(0);

    let refusals = [
        signer
            .release()
            .verify(&bytes, &stranger.sign(&bytes))
            .err(),
        staged(vec![7; 65]),
        signer
            .release()
            .on("amd64")
            .verify(&bytes, &signer.sign(&bytes))
            .err(),
        staged(half),
    ]
    .map(|refused| refused.and_then(|refused| refused.toolbox_refusal()));

    assert_eq!(
        refusals,
        [
            Some(ToolboxRefusal::Signature),
            Some(ToolboxRefusal::Length),
            Some(ToolboxRefusal::Architecture),
            Some(ToolboxRefusal::Digest),
        ]
    );
    assert!(recorder.mounted.lock().unwrap().is_empty(), "0 mounts");
    assert_eq!(toolboxes.digests(), [] as [String; 0]);
}

/// Dimension 8.4: a held image is never unmounted, the current and previous
/// are kept, and a runner killed mid-stage admits cleanly on restart.
#[test]
fn test_toolbox_holds_and_retention() {
    let dir = tempfile::tempdir().unwrap();
    let signer = Signer::new();
    let [one, two, three, four] = [1, 2, 3, 4].map(|n| release(&signer, dir.path(), n));
    let partial = dir.path().join("images/.stage").join("killed.partial");
    fs::create_dir_all(partial.parent().unwrap()).unwrap();
    fs::write(&partial, b"half a copy").unwrap();
    let (toolboxes, recorder) = open(dir.path());
    assert!(
        !partial.exists(),
        "the restart swept the killed run's stage"
    );

    let first = toolboxes.admit(&one.manifest, &one.source).unwrap();
    let (lease, slot) = (Arc::clone(&first), Arc::clone(&first));
    drop(first);
    toolboxes.admit(&two.manifest, &two.source).unwrap();
    toolboxes.admit(&three.manifest, &three.source).unwrap();
    drop(lease);
    toolboxes.retain().unwrap();
    let held_by_the_slot = toolboxes.digests();
    drop(slot);
    let released = toolboxes.retain().unwrap();
    let after_retain = recorder.unmounted.lock().unwrap().clone();
    toolboxes.admit(&four.manifest, &four.source).unwrap();

    let digest = |release: &Release| release.manifest.sha256().to_owned();
    assert_eq!(
        held_by_the_slot,
        [digest(&one), digest(&two), digest(&three)],
        "two holds, one released: still mounted"
    );
    assert_eq!(released, 1, "the last hold gone, the image goes");
    assert_eq!(after_retain, [digest(&one)], "and goes at that retain");
    assert_eq!(
        *recorder.unmounted.lock().unwrap(),
        [digest(&one), digest(&two)],
        "a fourth image: the oldest unheld goes"
    );
    assert_eq!(toolboxes.digests(), [digest(&three), digest(&four)]);
    let images = dir.path().join("images");
    assert!(!images.join(image_name(&digest(&one))).exists());
    assert!(images.join(image_name(&digest(&four))).exists());
    assert!(
        fs::read_dir(images.join(".stage"))
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn should_keep_an_image_whose_unmount_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let signer = Signer::new();
    let [one, two, three] = [1, 2, 3].map(|n| release(&signer, dir.path(), n));
    let (toolboxes, recorder) = open(dir.path());
    toolboxes.admit(&one.manifest, &one.source).unwrap();
    toolboxes.admit(&two.manifest, &two.source).unwrap();
    recorder.refuse_unmounts.store(true, Ordering::SeqCst);

    toolboxes.admit(&three.manifest, &three.source).unwrap();
    let refused = toolboxes.retain().unwrap_err();

    assert_eq!(refused.toolbox_refusal(), None, "the unmount's refusal");
    assert_eq!(
        toolboxes.digests().len(),
        3,
        "the new release is admitted, and nothing is forgotten"
    );
    toolboxes.close().unwrap_err();
    assert_eq!(
        toolboxes.digests().len(),
        3,
        "a close the kernel refuses forgets nothing"
    );
    recorder.refuse_unmounts.store(false, Ordering::SeqCst);
    assert_eq!(toolboxes.retain().unwrap(), 1);
    toolboxes.close().unwrap();
    assert_eq!(toolboxes.digests(), [] as [String; 0]);
}

#[test]
fn should_admit_a_published_image_without_staging_it_again() {
    let dir = tempfile::tempdir().unwrap();
    let signer = Signer::new();
    let one = release(&signer, dir.path(), 1);
    let (toolboxes, recorder) = open(dir.path());
    let first = toolboxes.admit(&one.manifest, &one.source).unwrap();
    fs::remove_file(&one.source).unwrap();

    let again = toolboxes
        .admit(&one.manifest, &dir.path().join("gone"))
        .unwrap();

    assert!(
        Arc::ptr_eq(&first, &again),
        "the admitted image, not a second"
    );
    assert_eq!(recorder.mounted.lock().unwrap().len(), 1);
}

#[test]
fn should_release_an_image_already_removed_and_keep_one_it_cannot_remove() {
    let dir = tempfile::tempdir().unwrap();
    let signer = Signer::new();
    let [one, two, three, four] = [1, 2, 3, 4].map(|n| release(&signer, dir.path(), n));
    let (toolboxes, recorder) = open(dir.path());
    let images = dir.path().join("images");
    let held = [&one, &two, &three]
        .map(|release| toolboxes.admit(&release.manifest, &release.source).unwrap());
    fs::remove_file(images.join(image_name(one.manifest.sha256()))).unwrap();
    let stuck = images.join(image_name(two.manifest.sha256()));
    fs::remove_file(&stuck).unwrap();
    fs::create_dir(&stuck).unwrap();
    fs::write(stuck.join("in the way"), b"").unwrap();
    drop(held);

    toolboxes.admit(&four.manifest, &four.source).unwrap();
    let refused = toolboxes.retain().unwrap_err();

    assert_eq!(refused.toolbox_refusal(), None, "the removal's own failure");
    assert_eq!(
        *recorder.unmounted.lock().unwrap(),
        [one.manifest.sha256()],
        "an image already gone is released; one whose file stays is not unmounted"
    );
    assert_eq!(
        toolboxes.digests(),
        [
            two.manifest.sha256(),
            three.manifest.sha256(),
            four.manifest.sha256()
        ],
        "the one whose image would not go stays admitted"
    );
}

/// A published image that is no longer the manifest's, rotted or edited in
/// place, is removed and staged again from the download, once; it does not
/// refuse every later admission of its release.
#[test]
fn should_stage_again_a_published_image_that_is_not_the_manifests() {
    let dir = tempfile::tempdir().unwrap();
    let signer = Signer::new();
    let one = release(&signer, dir.path(), 1);
    let (toolboxes, recorder) = open(dir.path());
    toolboxes.admit(&one.manifest, &one.source).unwrap();
    toolboxes.close().unwrap();
    let published = dir
        .path()
        .join("images")
        .join(image_name(one.manifest.sha256()));
    fs::write(&published, vec![9_u8; 64]).unwrap();

    toolboxes.admit(&one.manifest, &one.source).unwrap();

    assert_eq!(fs::read(&published).unwrap(), vec![1_u8; 64]);
    assert_eq!(recorder.mounted.lock().unwrap().len(), 2);
    fs::write(&one.source, vec![9_u8; 64]).unwrap();
    toolboxes.close().unwrap();
    fs::write(&published, vec![9_u8; 64]).unwrap();
    assert_eq!(
        toolboxes
            .admit(&one.manifest, &one.source)
            .unwrap_err()
            .toolbox_refusal(),
        Some(ToolboxRefusal::Digest),
        "a download that is not the image either is refused, once"
    );
}
