use std::io::Write as _;

use flate2::Compression;
use flate2::write::GzEncoder;

use super::super::{MAX_ENTRY_BYTES, MAX_TAR_ENTRIES, extract};
use crate::{Error, SourceFailure};

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(bytes).expect("fixture gzip writes");
    encoder.finish().expect("fixture gzip finishes")
}

#[test]
fn archives_enforce_entry_count_and_entry_size_ceilings() {
    let mut tar = tar::Builder::new(Vec::new());
    for index in 0..=MAX_TAR_ENTRIES {
        let path = format!("wrapper/file-{index}");
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, path, &[][..])
            .expect("fixture entry writes");
    }
    let compressed = gzip(&tar.into_inner().expect("fixture tar finishes"));
    assert!(matches!(
        extract(&compressed, "agentsfleet/reviewer", "main"),
        Err(Error::Source(SourceFailure::TooManyFiles))
    ));

    let content = vec![0_u8; usize::try_from(MAX_ENTRY_BYTES + 1).expect("limit fits")];
    let compressed = super::archive(&[("wrapper/SKILL.md", &content)]);
    assert!(matches!(
        extract(&compressed, "agentsfleet/reviewer", "main"),
        Err(Error::Source(SourceFailure::ArchiveTooLarge))
    ));
}
