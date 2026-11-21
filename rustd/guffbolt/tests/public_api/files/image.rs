//! `image` attaches a picture the wire takes, scales one longer than
//! 2,048 px on a side down to fit, and refuses the rest.

use ::image::codecs::gif::GifEncoder;
use ::image::codecs::jpeg::JpegEncoder;
use ::image::codecs::png::PngEncoder;
use ::image::codecs::webp::WebPEncoder;
use ::image::{ExtendedColorType, ImageEncoder, ImageFormat, Rgba, RgbaImage};

use super::*;

/// The longest side the model is shown: 2,048 px.
const MAX_SIDE: u32 = 2048;

/// A `width` by `height` picture in two colours, so a scaled one is not one
/// flat colour.
fn picture(width: u32, height: u32) -> RgbaImage {
    RgbaImage::from_fn(width, height, |x, _y| {
        if x % 2 == 0 {
            Rgba([200, 40, 40, 255])
        } else {
            Rgba([40, 40, 200, 255])
        }
    })
}

/// A `width` by `height` picture no compressor shrinks: each pixel's four
/// channels from a xorshift, so its PNG weighs about its raw size.
fn noise(width: u32, height: u32) -> RgbaImage {
    let mut state = 0x9e37_79b9_u32;
    RgbaImage::from_fn(width, height, |_x, _y| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        Rgba(state.to_le_bytes())
    })
}

/// `pixels` written as `kind`, the way a file on disk holds it.
fn encoded(pixels: &RgbaImage, kind: ImageKind) -> Vec<u8> {
    let mut bytes = Vec::new();
    let (width, height) = pixels.dimensions();
    match kind {
        ImageKind::Png => PngEncoder::new(&mut bytes)
            .write_image(pixels.as_raw(), width, height, ExtendedColorType::Rgba8)
            .expect("a PNG"),
        ImageKind::Jpeg => {
            let rgb = ::image::DynamicImage::ImageRgba8(pixels.clone()).to_rgb8();
            JpegEncoder::new_with_quality(&mut bytes, 90)
                .write_image(rgb.as_raw(), width, height, ExtendedColorType::Rgb8)
                .expect("a JPEG");
        }
        ImageKind::Gif => GifEncoder::new(&mut bytes)
            .encode(pixels.as_raw(), width, height, ExtendedColorType::Rgba8)
            .expect("a GIF"),
        ImageKind::Webp => WebPEncoder::new_lossless(&mut bytes)
            .write_image(pixels.as_raw(), width, height, ExtendedColorType::Rgba8)
            .expect("a WebP"),
    }
    bytes
}

/// The width and height `bytes`, a `format` picture, decode to.
fn dimensions(bytes: &[u8], format: ImageFormat) -> (u32, u32) {
    let decoded = ::image::load_from_memory_with_format(bytes, format).expect("it decodes");
    (decoded.width(), decoded.height())
}

#[tokio::test]
async fn image_attaches_each_kind_the_wires_take() {
    let desk = Desk::new();
    let kinds = [
        ("p.png", ImageKind::Png),
        ("j.jpg", ImageKind::Jpeg),
        ("g.gif", ImageKind::Gif),
        ("w.webp", ImageKind::Webp),
    ];
    let files = kinds.map(|(path, kind)| (path, kind, encoded(&picture(3, 2), kind)));
    for (path, _kind, bytes) in &files {
        desk.put(path, bytes);
    }
    let tools = desk.tools(true);

    for (path, kind, bytes) in files {
        let output = call(&tools, "image", json!({"path": path})).await;

        assert_eq!(
            output.text,
            format!(
                "Attached {path} ({} bytes, {}) for your next step",
                bytes.len(),
                kind.media_type()
            )
        );
        let image = output.image.expect("an image rides the output");
        assert_eq!(image.kind, kind, "{path}");
        assert_eq!(
            image.data.as_ref(),
            bytes.as_slice(),
            "{path} as the file holds it"
        );
    }
}

/// A picture at the bound is not scaled: the model gets the file's bytes.
#[tokio::test]
async fn an_image_within_the_bound_passes_untouched() {
    let desk = Desk::new();
    let bytes = encoded(&picture(MAX_SIDE, 8), ImageKind::Png);
    desk.put("wide.png", &bytes);
    let tools = desk.tools(true);

    let output = call(&tools, "image", json!({"path": "wide.png"})).await;

    assert_eq!(
        output.text,
        format!(
            "Attached wide.png ({} bytes, image/png) for your next step",
            bytes.len()
        )
    );
    let image = output.image.expect("an image rides the output");
    assert_eq!(image.data.as_ref(), bytes.as_slice());
}

/// A picture longer than the bound is scaled to fit, its aspect ratio kept.
#[tokio::test]
async fn an_image_over_the_bound_is_resized_to_2048() {
    let desk = Desk::new();
    desk.put("wide.png", encoded(&picture(4096, 1024), ImageKind::Png));
    let tools = desk.tools(true);

    let output = call(&tools, "image", json!({"path": "wide.png"})).await;

    let image = output.image.expect("an image rides the output");
    assert_eq!(image.kind, ImageKind::Png);
    assert_eq!(dimensions(&image.data, ImageFormat::Png), (2048, 512));
    assert_eq!(
        output.text,
        format!(
            "Attached wide.png ({} bytes, image/png, scaled from 4096x1024 to 2048x512) for your \
             next step",
            image.data.len()
        )
    );
}

/// A scaled PNG, JPEG or WebP is written again as its own kind; a scaled GIF
/// is written as a PNG.
#[tokio::test]
async fn a_scaled_image_keeps_its_kind_but_a_gif_becomes_a_png() {
    let desk = Desk::new();
    let kinds = [
        ("p.png", ImageKind::Png, ImageKind::Png, ImageFormat::Png),
        ("j.jpg", ImageKind::Jpeg, ImageKind::Jpeg, ImageFormat::Jpeg),
        ("g.gif", ImageKind::Gif, ImageKind::Png, ImageFormat::Png),
        (
            "w.webp",
            ImageKind::Webp,
            ImageKind::Webp,
            ImageFormat::WebP,
        ),
    ];
    for (path, kind, _written, _format) in kinds {
        desk.put(path, encoded(&picture(MAX_SIDE * 2, 8), kind));
    }
    let tools = desk.tools(true);

    for (path, _kind, written, format) in kinds {
        let output = call(&tools, "image", json!({"path": path})).await;

        let image = output.image.expect("an image rides the output");
        assert_eq!(image.kind, written, "{path}");
        assert_eq!(dimensions(&image.data, format), (MAX_SIDE, 4), "{path}");
        assert!(
            output.text.contains(written.media_type()),
            "{path}: {}",
            output.text
        );
    }
}

#[tokio::test]
async fn an_image_that_does_not_decode_is_refused() {
    let desk = Desk::new();
    desk.put("bad.png", b"\x89PNG\r\n\x1a\nnot a header, not pixels");
    let tools = desk.tools(true);

    let output = call(&tools, "image", json!({"path": "bad.png"})).await;

    assert_eq!(
        output.text,
        "[not_an_image] bad.png does not decode as a PNG image"
    );
    assert_eq!(output.image, None);
}

/// The 5 MiB cap bounds what the model is shown: noise just over the bound,
/// small enough for one read, scales by one pixel and still weighs about its
/// raw seven MB.
#[tokio::test]
async fn an_image_still_over_the_cap_once_scaled_is_refused() {
    let desk = Desk::new();
    let bytes = encoded(&noise(MAX_SIDE + 1, 900), ImageKind::Png);
    assert!(
        bytes.len() > IMAGE_MAX && bytes.len() <= ONE_READ,
        "the source is over the cap and within one read: {}",
        bytes.len()
    );
    desk.put("noise.png", bytes);
    let tools = desk.tools(true);

    let output = call(&tools, "image", json!({"path": "noise.png"})).await;

    assert_eq!(
        output.text,
        "[file_too_large] noise.png is longer than 5242880 bytes, scaled from 2049x900 to \
         2048x900"
    );
    assert_eq!(output.image, None);
}

#[tokio::test]
async fn image_refuses_a_wire_without_images_and_a_file_that_is_not_one() {
    let desk = Desk::new();
    desk.put("p.png", b"\x89PNG\r\n\x1a\n");
    desk.put("t.txt", "plain text");
    desk.put("riff.wav", b"RIFF\x00\x00\x00\x00WAVEfmt ");
    desk.put("empty.png", "");
    let mut big = b"\x89PNG\r\n\x1a\n".to_vec();
    big.resize(ONE_READ + 1, 0);
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
        "[file_too_large] big.png is longer than 8388608 bytes"
    );
    assert_eq!(code(&missing), Some("file_not_found"));
    assert_eq!(code(&outside), Some("path_not_allowed"));
    assert_eq!(large.image, None);
}
