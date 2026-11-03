//! What the host's page cache keeps of an admitted toolbox. Its loop device
//! reads the image with direct I/O, and admission drops the hash read's
//! pages, so reading every file in the toolbox leaves the image file itself
//! nearly uncached on the host.

use std::fs::{self, File};
use std::os::fd::AsRawFd as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use afr_sandbox::{Toolbox, ToolboxHome};
use libtest_mimic::Failed;

use crate::budgets::drop_caches;
use crate::exhaustion::{DIO_ON, loop_dio};
use crate::filesystems::on_disk;
use crate::lane::Lane;
use crate::run::{expect, runtime};

/// `cachestat(2)`'s number. Linux 6.5 gave it 451 in the table every
/// architecture shares, and `libc` does not name it for these targets.
const SYS_CACHESTAT: libc::c_long = 451;
/// The image's files and each one's size: 8 MiB in all, so 1% of it is
/// still a count of pages rather than a fraction of one.
const FILES: usize = 8;
const FILE_BYTES: usize = 1 << 20;

/// Dimension 1.1 (`M237_002`): with the page cache dropped, an admitted
/// toolbox's loop device reports direct I/O, and reading every file in it
/// leaves under 1% of the image file cached on the host. Reading the image
/// itself afterwards caches it whole, which proves the count can see pages.
pub(crate) fn toolbox_reads_past_the_host_cache(lane: &Lane) -> Result<(), Failed> {
    let dir = tempfile::tempdir_in(on_disk(lane)?)?;
    let image = bulky_image(dir.path())?;
    let manifest = lane.signer.manifest_for(&image)?;
    let mounts = ToolboxHome::at(dir.path().to_owned()).mounts();
    drop_caches()?;

    let toolbox = runtime().block_on(Toolbox::admit(&manifest, &image, &mounts))?;
    let read = read_every_file(toolbox.root());
    let dio = loop_dio(toolbox.root());
    toolbox.unmount()?;
    let image_file = File::open(&image)?;
    let cached = cached_pages(&image_file)?;
    fs::read(&image)?;
    let whole = cached_pages(&image_file)?;

    expect(
        dio?.trim() == DIO_ON,
        "the toolbox's loop device reports direct I/O",
    )?;
    expect(
        read? >= FILES * FILE_BYTES,
        "every file in the toolbox was read",
    )?;
    expect(
        whole > 0 && cached * 100 < whole,
        format!("{cached} of the image's {whole} pages stayed in the host's cache"),
    )
}

/// An EROFS image of [`FILES`] files of random bytes, which nothing can
/// compress or share.
fn bulky_image(dir: &Path) -> Result<PathBuf, Failed> {
    let tree = dir.join("bulky-tree");
    fs::create_dir(&tree)?;
    let mut bytes = vec![0_u8; FILE_BYTES];
    for number in 0..FILES {
        aws_lc_rs::rand::fill(&mut bytes).map_err(|_unspecified| "no random bytes")?;
        fs::write(tree.join(format!("file-{number}")), &bytes)?;
    }
    let image = dir.join("bulky.erofs");
    let made = Command::new("mkfs.erofs")
        .arg("--quiet")
        .args(["-T", "0"])
        .arg(&image)
        .arg(&tree)
        .status()?;
    expect(made.success(), format!("mkfs.erofs {}", image.display()))?;
    Ok(image)
}

/// Reads every regular file under `dir`; says how many bytes.
fn read_every_file(dir: &Path) -> Result<usize, Failed> {
    let mut read = 0;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            read += read_every_file(&entry.path())?;
        } else if kind.is_file() {
            read += fs::read(entry.path())?.len();
        }
    }
    Ok(read)
}

/// How many of `file`'s pages the host's page cache holds. The kernel
/// counts them, so asking reads no page in.
fn cached_pages(file: &File) -> Result<u64, Failed> {
    // `struct cachestat_range`: offset 0 and length 0, the whole file.
    let range = [0_u64; 2];
    // `struct cachestat`: five counters, the first the cached pages.
    let mut stat = [0_u64; 5];
    // SAFETY: cachestat(2) reads one `struct cachestat_range`, two `__u64`s,
    // from `range` and writes one `struct cachestat`, five `__u64`s, into
    // `stat`. Both outlive the call, and `file` holds its descriptor open.
    let answered = unsafe {
        libc::syscall(
            SYS_CACHESTAT,
            file.as_raw_fd(),
            &raw const range,
            &raw mut stat,
            0,
        )
    };
    if answered != 0 {
        return Err(format!("cachestat: {}", std::io::Error::last_os_error()).into());
    }
    Ok(stat[0])
}
