#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use super::{STAGE_DIR, published, stage, sweep};
use crate::error::ToolboxRefusal;
use crate::toolbox::testing::Signer;

/// An image's bytes: long enough that half of it is a real half.
fn image() -> Vec<u8> {
    (0..4096_u32).flat_map(u32::to_le_bytes).collect()
}

/// Everything left in `dir`'s staging directory.
fn staged(dir: &Path) -> Vec<String> {
    fs::read_dir(dir.join(STAGE_DIR))
        .map(|entries| {
            entries
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn should_publish_an_image_whose_bytes_are_the_manifests() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("download");
    fs::write(&source, image()).unwrap();
    let manifest = Signer::new().manifest(&image());

    let image_path = stage(&manifest, &source, dir.path()).unwrap();

    assert_eq!(image_path, published(dir.path(), &manifest));
    assert_eq!(fs::read(&image_path).unwrap(), image());
    let mode = fs::metadata(&image_path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o444, "nobody writes a published image");
    assert!(staged(dir.path()).is_empty(), "{:?}", staged(dir.path()));
}

#[test]
fn should_refuse_an_image_a_byte_long_or_short_and_publish_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = Signer::new().manifest(&image());
    let mut long = image();
    long.push(0);
    let short = image().split_last().unwrap().1.to_vec();

    for bytes in [long, short] {
        let source = dir.path().join("download");
        fs::write(&source, bytes).unwrap();

        let refused = stage(&manifest, &source, dir.path()).unwrap_err();

        assert_eq!(refused.toolbox_refusal(), Some(ToolboxRefusal::Length));
        assert!(!published(dir.path(), &manifest).exists());
        assert!(staged(dir.path()).is_empty(), "{:?}", staged(dir.path()));
    }
}

/// A download preallocated to its full length and written only half way has
/// the manifest's length and not its bytes.
#[test]
fn should_refuse_a_half_written_image_of_the_right_length() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = Signer::new().manifest(&image());
    let mut half = image();
    let middle = half.len() / 2;
    half.get_mut(middle..).unwrap().fill(0);
    let source = dir.path().join("download");
    fs::write(&source, half).unwrap();

    let refused = stage(&manifest, &source, dir.path()).unwrap_err();

    assert_eq!(refused.toolbox_refusal(), Some(ToolboxRefusal::Digest));
    assert!(!published(dir.path(), &manifest).exists());
    assert_eq!(staged(dir.path()), [] as [String; 0]);
}

#[test]
fn should_refuse_a_source_it_cannot_read_and_publish_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = Signer::new().manifest(&image());

    let refused = stage(&manifest, &dir.path().join("absent"), dir.path()).unwrap_err();

    assert_eq!(refused.toolbox_refusal(), None, "an input/output failure");
    assert!(!published(dir.path(), &manifest).exists());
}

#[test]
fn should_sweep_the_partial_copies_a_killed_run_left() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(sweep(dir.path()).unwrap(), 0, "no staging directory yet");
    fs::create_dir(dir.path().join(STAGE_DIR)).unwrap();
    for name in ["a.partial", "b.partial"] {
        fs::write(dir.path().join(STAGE_DIR).join(name), b"half").unwrap();
    }

    assert_eq!(sweep(dir.path()).unwrap(), 2);
    assert_eq!(staged(dir.path()), [] as [String; 0]);
    assert_eq!(sweep(dir.path()).unwrap(), 0);
}

/// Two admissions of one release copy into files of their own: a copy in
/// flight under the digest is neither cut short nor removed by another.
#[test]
fn should_stage_beside_another_copy_of_the_same_release() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("download");
    fs::write(&source, image()).unwrap();
    let manifest = Signer::new().manifest(&image());
    fs::create_dir(dir.path().join(STAGE_DIR)).unwrap();
    let in_flight = dir
        .path()
        .join(STAGE_DIR)
        .join(format!("{}.partial", manifest.sha256()));
    fs::write(&in_flight, b"another admission's copy so far").unwrap();

    stage(&manifest, &source, dir.path()).unwrap();

    assert_eq!(
        fs::read(&in_flight).unwrap(),
        b"another admission's copy so far"
    );
    assert_eq!(staged(dir.path()).len(), 1, "only the other copy is left");
}

/// The sweep takes partial copies only: a directory or a file it did not
/// name is left where it is, and does not stop the runner from starting.
#[test]
fn should_sweep_partial_copies_and_leave_anything_else() {
    let dir = tempfile::tempdir().unwrap();
    let staging = dir.path().join(STAGE_DIR);
    fs::create_dir_all(staging.join("a-directory")).unwrap();
    fs::write(staging.join("notes.txt"), b"kept").unwrap();
    fs::write(staging.join("c.partial"), b"half").unwrap();

    assert_eq!(sweep(dir.path()).unwrap(), 1);
    let mut left = staged(dir.path());
    left.sort();
    assert_eq!(left, ["a-directory", "notes.txt"]);
}

#[test]
fn should_refuse_to_sweep_a_staging_path_it_cannot_read() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(STAGE_DIR), b"a file, not a directory").unwrap();

    sweep(dir.path()).unwrap_err();
}
