//! `image` attaches a picture the wire takes and refuses the rest.

use super::*;

#[tokio::test]
async fn image_attaches_each_kind_the_wires_take() {
    let desk = Desk::new();
    desk.put("p.png", b"\x89PNG\r\n\x1a\nrest");
    desk.put("j.jpg", [0xff_u8, 0xd8, 0xff, 0xe0]);
    desk.put("g.gif", b"GIF89a");
    desk.put("w.webp", b"RIFF\x00\x00\x00\x00WEBPVP8 ");
    let tools = desk.tools(true);

    let mut seen = Vec::new();
    for path in ["p.png", "j.jpg", "g.gif", "w.webp"] {
        let output = call(&tools, "image", json!({"path": path})).await;
        seen.push((
            output.text,
            output.image.map(|image| (image.kind, image.data.len())),
        ));
    }

    assert_eq!(
        seen,
        [
            (
                "Attached p.png (12 bytes, image/png) for your next step".to_owned(),
                Some((ImageKind::Png, 12))
            ),
            (
                "Attached j.jpg (4 bytes, image/jpeg) for your next step".to_owned(),
                Some((ImageKind::Jpeg, 4))
            ),
            (
                "Attached g.gif (6 bytes, image/gif) for your next step".to_owned(),
                Some((ImageKind::Gif, 6))
            ),
            (
                "Attached w.webp (16 bytes, image/webp) for your next step".to_owned(),
                Some((ImageKind::Webp, 16))
            ),
        ]
    );
}

#[tokio::test]
async fn image_refuses_a_wire_without_images_and_a_file_that_is_not_one() {
    let desk = Desk::new();
    desk.put("p.png", b"\x89PNG\r\n\x1a\n");
    desk.put("t.txt", "plain text");
    desk.put("riff.wav", b"RIFF\x00\x00\x00\x00WAVEfmt ");
    desk.put("empty.png", "");
    let mut big = b"\x89PNG\r\n\x1a\n".to_vec();
    big.resize(IMAGE_MAX + 1, 0);
    desk.put("big.png", big);
    let blind = desk.tools(false);
    let tools = desk.tools(true);

    let no_wire = call(&blind, "image", json!({"path": "p.png"})).await;
    let text = call(&tools, "image", json!({"path": "t.txt"})).await;
    let wave = call(&tools, "image", json!({"path": "riff.wav"})).await;
    let empty = call(&tools, "image", json!({"path": "empty.png"})).await;
    let large = call(&tools, "image", json!({"path": "big.png"})).await;
    let missing = call(&tools, "image", json!({"path": "nowhere.png"})).await;
    let outside = call(&tools, "image", json!({"path": "../p.png"})).await;

    assert_eq!(
        no_wire.text,
        "[image_input_unavailable] this model's wire takes no image in a tool result"
    );
    assert_eq!(
        text.text,
        "[not_an_image] t.txt is not a PNG, JPEG, GIF or WebP image"
    );
    assert_eq!(
        wave.text,
        "[not_an_image] riff.wav is not a PNG, JPEG, GIF or WebP image"
    );
    assert_eq!(
        empty.text,
        "[not_an_image] empty.png is not a PNG, JPEG, GIF or WebP image"
    );
    assert_eq!(
        large.text,
        "[file_too_large] big.png is longer than 5242880 bytes"
    );
    assert_eq!(code(&missing), Some("file_not_found"));
    assert_eq!(code(&outside), Some("path_not_allowed"));
    assert_eq!(large.image, None);
}
