#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use serde_json::json;

use super::{IMAGE_MAX_BYTES, ImageAttachment, ImageKind};
use crate::catalog::IMAGE;
use crate::lease::Lease;
use crate::runtime::{ToolErrorCode, ToolOutput};
use crate::sandbox::ScriptedExecutor;
use crate::testing::{Live, call, call_in, hosted, offered};

/// The argument name the calls spell.
const PATH: &str = "path";
/// A screenshot's size: forty kibibytes.
const SHOT_BYTES: usize = 40 * 1024;
/// The files the suite plants.
const SHOT: &str = "shot.png";
const BIG: &str = "big.png";
const NOTES: &str = "notes.txt";

/// `kind`'s first bytes, padded with zeros to `length`.
fn bytes_of(kind: ImageKind, length: usize) -> Vec<u8> {
    let mut bytes = match kind {
        ImageKind::Png => super::PNG_MAGIC.to_vec(),
        ImageKind::Jpeg => super::JPEG_MAGIC.to_vec(),
        ImageKind::Gif => b"GIF89a".to_vec(),
        ImageKind::Webp => b"RIFF\0\0\0\0WEBPVP8 ".to_vec(),
    };
    bytes.resize(length, 0);
    bytes
}

/// Calls `image` on `path` through `live`, with a lease whose wire takes
/// images or not; the answer and what the lease holds after it.
async fn viewed(live: &Live, path: &str, images: bool) -> (ToolOutput, Option<ImageAttachment>) {
    let (catalog, _sent) = hosted();
    let selection = catalog.select(&[IMAGE.name()]).unwrap();
    let mut lease = Lease::default().with_image_input(images);
    let answer = call_in(
        offered(&selection, &IMAGE),
        &live.client,
        &mut lease,
        json!({PATH: path}),
    )
    .await;
    (answer, lease.attachment)
}

#[tokio::test]
async fn test_image_refusals_carry_codes() {
    let live = Live::start().await;
    let oversize = usize::try_from(IMAGE_MAX_BYTES).unwrap() * 4;
    std::fs::write(live.root.join(BIG), bytes_of(ImageKind::Png, oversize)).unwrap();
    std::fs::write(live.root.join(NOTES), b"not pixels").unwrap();
    std::fs::write(live.root.join(SHOT), bytes_of(ImageKind::Png, SHOT_BYTES)).unwrap();

    let (too_big, held_big) = viewed(&live, BIG, true).await;
    let (not_image, held_text) = viewed(&live, NOTES, true).await;
    let (no_input, held_chat) = viewed(&live, SHOT, false).await;

    assert_eq!(
        too_big.error_code,
        Some(ToolErrorCode::FileTooLarge),
        "{too_big:?}"
    );
    assert_eq!(
        not_image.error_code,
        Some(ToolErrorCode::NotAnImage),
        "{not_image:?}"
    );
    assert_eq!(
        no_input.error_code,
        Some(ToolErrorCode::ImageInputUnavailable),
        "{no_input:?}"
    );
    assert!(
        too_big.text.contains(&IMAGE_MAX_BYTES.to_string()),
        "{}",
        too_big.text
    );
    assert!(not_image.text.contains("not a PNG"), "{}", not_image.text);
    assert!(
        no_input.text.contains("takes no image"),
        "{}",
        no_input.text
    );
    assert!(
        held_big.is_none() && held_text.is_none() && held_chat.is_none(),
        "a refused call attaches nothing"
    );
    live.stop().await;
}

/// The bytes wait on the lease; the text the thread keeps names the path,
/// the size and the kind, and none of the bytes.
#[tokio::test]
async fn an_image_is_attached_to_the_lease_and_the_text_names_no_bytes() {
    let live = Live::start().await;
    let shot = bytes_of(ImageKind::Png, SHOT_BYTES);
    std::fs::write(live.root.join(SHOT), &shot).unwrap();

    let (answer, held) = viewed(&live, SHOT, true).await;

    assert_eq!(answer.error_code, None, "{answer:?}");
    assert_eq!(
        answer.text,
        "Attached shot.png (40960 bytes, image/png) for your next turn"
    );
    let held = held.unwrap();
    assert_eq!(held.kind, ImageKind::Png);
    assert_eq!(held.bytes.as_ref(), shot.as_slice());
    live.stop().await;
}

#[tokio::test]
async fn a_jpeg_a_gif_and_a_webp_attach_under_their_kinds() {
    let live = Live::start().await;
    for (kind, name) in [
        (ImageKind::Jpeg, "photo.jpg"),
        (ImageKind::Gif, "loop.gif"),
        (ImageKind::Webp, "web.webp"),
    ] {
        std::fs::write(live.root.join(name), bytes_of(kind, 64)).unwrap();

        let (answer, held) = viewed(&live, name, true).await;

        assert_eq!(answer.error_code, None, "{answer:?}");
        assert!(answer.text.contains(kind.mime()), "{}", answer.text);
        assert_eq!(held.map(|held| held.kind), Some(kind));
    }
    live.stop().await;
}

#[test]
fn each_kind_is_told_by_its_first_bytes() {
    // pin test: literal is the contract
    assert_eq!(
        ImageKind::sniffed(b"\x89PNG\r\n\x1a\nrest"),
        Some(ImageKind::Png)
    );
    // pin test: literal is the contract
    assert_eq!(
        ImageKind::sniffed(&[0xff, 0xd8, 0xff, 0xe0]),
        Some(ImageKind::Jpeg)
    );
    // pin test: literal is the contract
    assert_eq!(ImageKind::sniffed(b"GIF87a"), Some(ImageKind::Gif));
    // pin test: literal is the contract
    assert_eq!(
        ImageKind::sniffed(b"RIFF\x10\0\0\0WEBPVP8 "),
        Some(ImageKind::Webp)
    );
    assert_eq!(
        ImageKind::sniffed(b"RIFF\x10\0\0\0WAVEfmt "),
        None,
        "a RIFF that is not WebP"
    );
    assert_eq!(
        ImageKind::sniffed(b"RIFF"),
        None,
        "a RIFF cut before its word"
    );
    assert_eq!(ImageKind::sniffed(b""), None);
    assert_eq!(
        ImageKind::sniffed(b"<svg xmlns"),
        None,
        "the wires take no SVG from a tool"
    );
    assert_eq!(
        [
            ImageKind::Png,
            ImageKind::Jpeg,
            ImageKind::Gif,
            ImageKind::Webp
        ]
        .map(ImageKind::mime),
        ["image/png", "image/jpeg", "image/gif", "image/webp"]
    );
}

#[tokio::test]
async fn a_missing_image_a_path_out_and_no_sandbox_read_their_codes() {
    let live = Live::start().await;
    let scripted = ScriptedExecutor::default();
    let (catalog, _sent) = hosted();
    let selection = catalog.select(&[IMAGE.name()]).unwrap();
    let tool = offered(&selection, &IMAGE);
    let mut lease = Lease::default().with_image_input(true);

    let missing = call_in(tool, &live.client, &mut lease, json!({PATH: "absent.png"})).await;
    let out = call_in(tool, &scripted, &mut lease, json!({PATH: "../etc/x.png"})).await;
    let unsandboxed = call(tool, &mut lease, json!({PATH: SHOT})).await;

    assert_eq!(
        missing.error_code,
        Some(ToolErrorCode::FileNotFound),
        "{missing:?}"
    );
    assert_eq!(
        out.error_code,
        Some(ToolErrorCode::PathNotAllowed),
        "{out:?}"
    );
    assert_eq!(
        unsandboxed.error_code,
        Some(ToolErrorCode::SandboxUnavailable),
        "{unsandboxed:?}"
    );
    assert!(
        scripted.spawned().is_empty(),
        "nothing was asked of the sandbox"
    );
    assert!(lease.attachment.is_none());
    live.stop().await;
}

/// The cap is the Messages wire's: five mebibytes per image.
#[test]
fn the_image_cap_is_five_mebibytes() {
    // pin test: literal is the contract
    assert_eq!(IMAGE_MAX_BYTES, 5_242_880);
}
