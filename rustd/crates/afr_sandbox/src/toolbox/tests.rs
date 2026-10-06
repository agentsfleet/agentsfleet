use std::path::PathBuf;

use super::{TOOLBOX_PREFIX, TOOLBOX_SUFFIX, Toolbox, image_name};

#[test]
fn an_image_is_published_under_its_digest() {
    let name = image_name("abc123");

    assert_eq!(name, "toolbox-abc123.erofs");
    assert!(name.starts_with(TOOLBOX_PREFIX) && name.ends_with(TOOLBOX_SUFFIX));
}

#[test]
fn an_adopted_root_is_used_as_given() {
    let toolbox = Toolbox::at(PathBuf::from("/srv/root"), "abc".to_owned());

    assert_eq!(toolbox.root(), PathBuf::from("/srv/root"));
    assert_eq!(toolbox.digest(), "abc");
}

/// Admission reads the image through one descriptor and refuses before any
/// loop device or mount is needed, so these run unprivileged.
#[cfg(target_os = "linux")]
mod refusals {
    #![expect(
        clippy::unwrap_used,
        reason = "test module: a failed precondition should fail the test loudly"
    )]

    use std::fs;
    use std::os::unix::fs::symlink;

    use crate::error::ToolboxRefusal;
    use crate::toolbox::Toolbox;
    use crate::toolbox::testing::Signer;

    const IMAGE: &[u8] = b"an image";

    fn refusal(image: &std::path::Path) -> Option<ToolboxRefusal> {
        let mounts = tempfile::tempdir().unwrap();
        let manifest = Signer::new().manifest(IMAGE);
        Toolbox::admit_now(&manifest, image, mounts.path())
            .err()
            .and_then(|refused| refused.toolbox_refusal())
    }

    #[test]
    fn should_refuse_a_link_a_directory_a_pipe_a_wrong_length_or_wrong_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        fs::write(&real, IMAGE).unwrap();
        let link = dir.path().join("link");
        symlink(&real, &link).unwrap();
        let pipe = dir.path().join("pipe");
        rustix::fs::mknodat(
            rustix::fs::CWD,
            &pipe,
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
            0,
        )
        .unwrap();
        let long = dir.path().join("long");
        fs::write(&long, b"an image!").unwrap();
        let other = dir.path().join("other");
        fs::write(&other, b"an imagE").unwrap();

        assert_eq!(refusal(&link), Some(ToolboxRefusal::NotAFile));
        assert_eq!(refusal(dir.path()), Some(ToolboxRefusal::NotAFile));
        assert_eq!(refusal(&pipe), Some(ToolboxRefusal::NotAFile));
        assert_eq!(refusal(&long), Some(ToolboxRefusal::Length));
        assert_eq!(refusal(&other), Some(ToolboxRefusal::Digest));
    }
}
