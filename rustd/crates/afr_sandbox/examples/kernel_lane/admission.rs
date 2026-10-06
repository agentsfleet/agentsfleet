//! Admission by descriptor on a real kernel: a path swapped mid-admission
//! never gets its file mounted, and a mount found at a digest's directory is
//! adopted only when it is the admitted file, mounted the way admission does.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use afr_sandbox::{Manifest, Toolbox, ToolboxRefusal};
use libtest_mimic::Failed;
use rustix::fs::{CWD, RenameFlags};

use crate::lane::Lane;
use crate::release::{INNER, MARKER, small_image};
use crate::run::{expect, runtime};

/// Admissions raced against the swapping path.
const ADMISSIONS: usize = 1000;
/// What the authorized image and the decoy say in their marker file.
const AUTHORIZED: &str = "authorized";
const DECOY: &str = "decoy";
/// The size of the foreign ext4 file system planted at a digest's directory.
const EXT4_BYTES: u64 = 8 * 1024 * 1024;
/// Where, under a trial's directory, its images are mounted.
const MOUNTS: &str = "mounts";
/// Where the kernel lists this process's mounts.
const MOUNTINFO: &str = "/proc/self/mountinfo";
/// The mount program, and the arguments the trials pass it: options, a file
/// system's type, and the options admission mounts a toolbox with.
const MOUNT: &str = "mount";
const OPTIONS: &str = "-o";
const TYPE: &str = "-t";
const TMPFS: &str = "tmpfs";
const AS_ADMITTED: &str = "loop,ro,nosuid,nodev";

/// Admits `image` under `mounts` on a fresh runtime.
fn admit(manifest: &Manifest, image: &Path, mounts: &Path) -> afr_sandbox::Result<Toolbox> {
    runtime().block_on(Toolbox::admit(manifest, image, mounts))
}

/// What the mounted toolbox's marker file says.
fn marker(toolbox: &Toolbox) -> Result<String, Failed> {
    Ok(fs::read_to_string(toolbox.root().join(MARKER))?)
}

/// Dimension 8.1: across 1,000 admissions while another thread exchanges the
/// published path with a decoy as fast as it can, only the authorized image
/// is ever mounted; an admission that opened the decoy refuses it.
pub(crate) fn path_swap(lane: &Lane) -> Result<(), Failed> {
    let dir = tempfile::tempdir_in("/tmp")?;
    let published = small_image(dir.path(), AUTHORIZED)?;
    let decoy = small_image(dir.path(), DECOY)?;
    let manifest = lane.signer.manifest_for(&published)?;
    let mounts = dir.path().join(MOUNTS);
    let stop = Arc::new(AtomicBool::new(false));
    let racer = {
        let (published, decoy, stop) = (published.clone(), decoy.clone(), Arc::clone(&stop));
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let _swapped =
                    rustix::fs::renameat_with(CWD, &published, CWD, &decoy, RenameFlags::EXCHANGE);
            }
        })
    };
    let (mut mounted, mut refused) = (0, 0);
    let mut outcome = Ok(());
    for _ in 0..ADMISSIONS {
        match admit(&manifest, &published, &mounts) {
            Ok(toolbox) => {
                let said = marker(&toolbox);
                toolbox.unmount()?;
                if said? != AUTHORIZED {
                    outcome = Err(Failed::from("the decoy was mounted"));
                    break;
                }
                mounted += 1;
            }
            Err(error) if is_decoy(&error) => refused += 1,
            Err(error) => {
                outcome = Err(error.into());
                break;
            }
        }
    }
    stop.store(true, Ordering::Relaxed);
    racer
        .join()
        .map_err(|_panicked| Failed::from("the racer panicked"))?;
    outcome?;
    println!(
        "{ADMISSIONS} admissions: {mounted} mounted the authorized image, {refused} refused the decoy, 0 mounted it"
    );
    expect(
        mounted > 0 && refused > 0,
        format!("the swap must land both ways, got {mounted} mounted and {refused} refused"),
    )
}

/// Whether admission refused a file that was not the manifest's.
fn is_decoy(error: &afr_sandbox::Error) -> bool {
    matches!(
        error.toolbox_refusal(),
        Some(ToolboxRefusal::Digest | ToolboxRefusal::Length)
    )
}

/// Dimension 8.3: an ext4 file system, the image itself mounted without
/// `nosuid` or `nodev`, a loop device of a copy of the image, a bind of a
/// directory inside the image, and a matching mount with another stacked
/// beneath it, each found at the digest's directory, are detached and the
/// admitted file is mounted in their place; a mount that is the admitted
/// file, mounted that way, is adopted as it is.
pub(crate) fn adoption(lane: &Lane) -> Result<(), Failed> {
    let dir = tempfile::tempdir_in("/tmp")?;
    let image = small_image(dir.path(), AUTHORIZED)?;
    let manifest = lane.signer.manifest_for(&image)?;
    let mounts = dir.path().join(MOUNTS);
    let root = mounts.join(manifest.sha256());
    let ext4 = dir.path().join("foreign.ext4");
    fs::File::create(&ext4)?.set_len(EXT4_BYTES)?;
    run("mke2fs", &["-q", TYPE, "ext4", "-F", &text(&ext4)])?;
    let copy = dir.path().join("copy.erofs");
    fs::copy(&image, &copy)?;

    for (foreign, options) in [(&ext4, "loop"), (&image, "loop,ro"), (&copy, AS_ADMITTED)] {
        fs::create_dir_all(&root)?;
        run(MOUNT, &[OPTIONS, options, &text(foreign), &text(&root)])?;
        replaced(&admit(&manifest, &image, &mounts)?, &image, &text(foreign))?;
    }
    partial_and_stacked(&manifest, &image, &mounts, dir.path())?;
    let first = admit(&manifest, &image, &mounts)?;
    let device = rustix::fs::stat(first.root())?.st_dev;
    let second = admit(&manifest, &image, &mounts)?;
    let adopted = rustix::fs::stat(second.root())?.st_dev == device;
    second.unmount()?;
    expect(adopted, "a matching mount is adopted, not mounted again")
}

/// A bind of a directory inside the admitted image, then a mount stacked
/// beneath a matching one: each passes every other check, and each is
/// detached and the image mounted afresh.
fn partial_and_stacked(
    manifest: &Manifest,
    image: &Path,
    mounts: &Path,
    scratch: &Path,
) -> Result<(), Failed> {
    let root = mounts.join(manifest.sha256());
    let whole = scratch.join("whole");
    fs::create_dir_all(&whole)?;
    fs::create_dir_all(&root)?;
    run(MOUNT, &[OPTIONS, AS_ADMITTED, &text(image), &text(&whole)])?;
    run(MOUNT, &["--bind", &text(&whole.join(INNER)), &text(&root)])?;
    let rebound = admit(manifest, image, mounts);
    run("umount", &[&text(&whole)])?;
    replaced(&rebound?, image, "a bind of a directory inside the image")?;
    let matching = admit(manifest, image, mounts)?;
    run(
        MOUNT,
        &[TYPE, TMPFS, TMPFS, &text(&matching.root().join(INNER))],
    )?;
    replaced(
        &admit(manifest, image, mounts)?,
        image,
        "a mount stacked beneath a matching one",
    )
}

/// Refuses unless `toolbox`, admitted over `foreign`, is the one mount at its
/// root and nothing is mounted beneath it; unmounts it either way.
fn replaced(toolbox: &Toolbox, image: &Path, foreign: &str) -> Result<(), Failed> {
    let found = toolbox_mount(toolbox.root(), image).and_then(|()| nothing_beneath(toolbox.root()));
    toolbox.unmount()?;
    found.map_err(|why| format!("over {foreign}: {}", why.message().unwrap_or_default()).into())
}

/// Refuses when anything is mounted under `root`.
fn nothing_beneath(root: &Path) -> Result<(), Failed> {
    let beneath = format!("{}/", fs::canonicalize(root)?.display());
    let listed = fs::read_to_string(MOUNTINFO)?;
    let stacked: Vec<&str> = listed
        .lines()
        .filter(|line| {
            line.split(' ')
                .nth(4)
                .is_some_and(|point| point.starts_with(&beneath))
        })
        .collect();
    expect(
        stacked.is_empty(),
        format!("mounts beneath it: {stacked:?}"),
    )
}

/// Refuses unless the one mount at `root` is a whole EROFS file system,
/// `ro,nosuid,nodev`, on a loop device whose backing file is `image`.
fn toolbox_mount(root: &Path, image: &Path) -> Result<(), Failed> {
    let root = fs::canonicalize(root)?;
    let listed = fs::read_to_string(MOUNTINFO)?;
    let at_root: Vec<&str> = listed
        .lines()
        .filter(|line| line.split(' ').nth(4) == Some(&*root.to_string_lossy()))
        .collect();
    let [line] = at_root.as_slice() else {
        return Err(format!("one mount at the root, got {at_root:?}").into());
    };
    let fields: Vec<&str> = line.split(' ').collect();
    let (options, kind) = (
        fields.get(5).copied().unwrap_or_default(),
        fields
            .iter()
            .skip_while(|field| **field != "-")
            .nth(1)
            .copied(),
    );
    expect(kind == Some("erofs"), format!("an erofs mount, got {line}"))?;
    expect(
        fields.get(3) == Some(&"/"),
        format!("the whole file system, got {line}"),
    )?;
    for option in ["ro", "nosuid", "nodev"] {
        expect(
            options.split(',').any(|set| set == option),
            format!("{option} in {line}"),
        )?;
    }
    let device = rustix::fs::stat(&root)?.st_dev;
    let backing = fs::read_to_string(format!(
        "/sys/dev/block/{}:{}/loop/backing_file",
        rustix::fs::major(device),
        rustix::fs::minor(device)
    ))?;
    expect(
        Path::new(backing.trim()) == fs::canonicalize(image)?,
        format!("backed by {}, got {backing:?}", image.display()),
    )
}

/// Runs `program` with `args`, refusing unless it succeeds.
fn run(program: &str, args: &[&str]) -> Result<(), Failed> {
    let status = Command::new(program).args(args).status()?;
    expect(status.success(), format!("{program} {args:?}: {status}"))
}

/// `path` as one argument.
fn text(path: &Path) -> String {
    path.display().to_string()
}
