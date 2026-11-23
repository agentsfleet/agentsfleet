//! `image`: a file the model looks at on its next step.
//!
//! The file's bytes stay out of the call's text, which the thread and the
//! trace keep, and ride the output for the loop to attach to this call's
//! result; what the model and the thread read is the path, the kind and the
//! byte count. A wire that cannot be shown an image refuses the call before
//! any read, and a file that is not one of the four kinds the wires take, or
//! does not decode as the one it begins as, is refused after it.
//!
//! A picture longer than the tool's
//! [`Limits::max_image_dimension`](crate::tools::Limits::max_image_dimension)
//! on either side is scaled to
//! fit and written again: the aspect ratio kept, the triangle filter, a PNG,
//! JPEG or WebP in its own kind and a GIF as a PNG, with its RGB color profile
//! and its EXIF data. One that already fits is passed on as the file holds it. A file is read whole
//! or refused, so one longer than one read carries ([`Env::max_read_bytes`]) is
//! not scaled. [`Limits::max_image_bytes`](crate::tools::Limits::max_image_bytes)
//! bounds what the model is shown, so a picture still heavier once scaled is
//! refused.

use std::io::Cursor;

use bytes::Bytes;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use image::imageops::FilterType;
use image::{
    DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageError, ImageFormat,
    ImageReader,
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{Answer, failed, inside, settled};
use crate::tool::ToolOutput;
use crate::tools::env::Env;
use crate::tools::{Context, Handler, Limits, Schema, code};
use guffbolt_ai::{Image, ImageKind};

/// How a PNG begins.
pub const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
/// How a JPEG begins.
pub const JPEG_MAGIC: &[u8] = &[0xff, 0xd8, 0xff];
/// How a GIF begins.
const GIF_MAGIC: &[u8] = b"GIF8";
const RIFF_MAGIC: &[u8] = b"RIFF";
/// What a RIFF container says at byte eight when it holds a WebP.
const WEBP_MAGIC: &[u8] = b"WEBP";
/// Where in a RIFF container that word sits.
const WEBP_MAGIC_AT: usize = 8;
/// What a call reads back when the model's wire takes no image.
const NO_IMAGE_INPUT: &str = "this model's wire takes no image in a tool result";
/// The quality a scaled JPEG is written at.
const JPEG_QUALITY: u8 = 85;
/// Where an ICC profile names its data's color space.
const ICC_COLOR_SPACE: core::ops::Range<usize> = 16..20;
/// The one color space a profile is kept for: every encoding below writes
/// RGB, and a JPEG decoded from CMYK keeps its source profile, which would
/// mislabel the pixels written.
const ICC_RGB: &[u8] = b"RGB ";

/// The kind of image the wires take that `bytes` begin as, if any.
#[must_use]
pub fn sniffed(bytes: &[u8]) -> Option<ImageKind> {
    let webp = bytes.starts_with(RIFF_MAGIC)
        && bytes
            .get(WEBP_MAGIC_AT..WEBP_MAGIC_AT + WEBP_MAGIC.len())
            .is_some_and(|word| word == WEBP_MAGIC);
    if bytes.starts_with(PNG_MAGIC) {
        Some(ImageKind::Png)
    } else if bytes.starts_with(JPEG_MAGIC) {
        Some(ImageKind::Jpeg)
    } else if bytes.starts_with(GIF_MAGIC) {
        Some(ImageKind::Gif)
    } else if webp {
        Some(ImageKind::Webp)
    } else {
        None
    }
}

/// `image`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Named {
    /// The image file, relative to the working directory: PNG, JPEG, GIF or
    /// WebP.
    path: String,
}

/// Shows the model a file.
#[derive(Debug, Clone, Copy)]
pub struct ImageTool<'run> {
    env: &'run dyn Env,
    /// Whether the model's wire takes an image with a call's result; the
    /// tool refuses before any read when it does not.
    image_input: bool,
    /// The bounds on what the model is shown.
    bounds: Bounds,
}

/// The longest side a picture is scaled to fit, and the most the scaled
/// picture may weigh.
#[derive(Debug, Clone, Copy)]
struct Bounds {
    max_dimension: u32,
    max_bytes: u64,
}

impl<'run> ImageTool<'run> {
    /// The tool, reading from `env` for a wire that takes an image with a
    /// call's result when `image_input` says so.
    #[must_use]
    pub const fn new(env: &'run dyn Env, image_input: bool) -> Self {
        Self {
            env,
            image_input,
            bounds: Bounds {
                max_dimension: Limits::DEFAULT.max_image_dimension,
                max_bytes: Limits::DEFAULT.max_image_bytes,
            },
        }
    }

    /// The same tool, a picture longer than `max_dimension` pixels on a side
    /// scaled to fit, and one weighing more than `max_bytes` after scaling
    /// refused.
    #[must_use]
    pub const fn with_bounds(mut self, max_dimension: u32, max_bytes: u64) -> Self {
        self.bounds = Bounds {
            max_dimension,
            max_bytes,
        };
        self
    }
}

impl Handler for ImageTool<'_> {
    const NAME: &'static str = "image";
    const DESCRIPTION: &'static str = "Look at an image file in the working directory: PNG, \
        JPEG, GIF or WebP. The image is shown to you with this call's result on your next \
        step.";
    type Arguments = Named;

    fn schema(&self) -> Schema {
        let side = self.bounds.max_dimension;
        let description = format!(
            "{} One longer than {side} pixels on a side is scaled down to fit.",
            Self::DESCRIPTION
        );
        Schema::of::<Named>(Self::DESCRIPTION).with_description(description)
    }

    async fn run(&self, arguments: Named, _context: Context<'_>) -> ToolOutput {
        settled(view(self.env, self.image_input, self.bounds, &arguments.path).await)
    }
}

async fn view(env: &dyn Env, image_input: bool, bounds: Bounds, path: &str) -> Answer {
    if !image_input {
        return Err(ToolOutput::failed(
            code::IMAGE_INPUT_UNAVAILABLE,
            NO_IMAGE_INPUT,
        ));
    }
    let path = inside(env, path)?;
    let most = env.max_read_bytes();
    let fetched = env
        .read_file(path, most)
        .await
        .map_err(|failure| failed(&failure))?;
    if fetched.truncated {
        return Err(ToolOutput::failed(
            code::FILE_TOO_LARGE,
            &format!("{path} is longer than {most} bytes"),
        ));
    }
    let kind = sniffed(&fetched.data).ok_or_else(|| {
        ToolOutput::failed(
            code::NOT_AN_IMAGE,
            &format!("{path} is not a PNG, JPEG, GIF or WebP image"),
        )
    })?;
    let shown = fitted(path, kind, fetched.data, bounds.max_dimension)?;
    let bytes = shown.image.data.len();
    let scaled = shown
        .scaled
        .map_or_else(String::new, |scaled| format!(", scaled from {scaled}"));
    let max_bytes = bounds.max_bytes;
    if u64::try_from(bytes).unwrap_or(u64::MAX) > max_bytes {
        return Err(ToolOutput::failed(
            code::FILE_TOO_LARGE,
            &format!("{path} is longer than {max_bytes} bytes{scaled}"),
        ));
    }
    let attached = ToolOutput::succeeded(format!(
        "Attached {path} ({bytes} bytes, {}{scaled}) for your next step",
        shown.image.kind.media_type()
    ));
    Ok(attached.with_image(shown.image))
}

/// The picture the model is shown, and how it was scaled when it was.
struct Shown {
    image: Image,
    scaled: Option<Scaled>,
}

/// A picture's size before and after it was scaled to fit.
#[derive(Debug, Clone, Copy)]
struct Scaled {
    from: (u32, u32),
    to: (u32, u32),
}

impl core::fmt::Display for Scaled {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let Self {
            from: (from_width, from_height),
            to: (to_width, to_height),
        } = *self;
        write!(f, "{from_width}x{from_height} to {to_width}x{to_height}")
    }
}

/// What a re-encoded picture carries over from its file: the color profile
/// and the EXIF data, which holds the orientation. Nothing else a format
/// keeps is copied.
struct Metadata {
    icc_profile: Option<Vec<u8>>,
    exif: Option<Vec<u8>>,
}

impl Metadata {
    /// Hands what it carries to `encoder` before the pixels are written.
    fn apply(self, encoder: &mut impl ImageEncoder) -> Result<(), ImageError> {
        if let Some(icc_profile) = self.icc_profile {
            encoder
                .set_icc_profile(icc_profile)
                .map_err(ImageError::Unsupported)?;
        }
        if let Some(exif) = self.exif {
            encoder
                .set_exif_metadata(exif)
                .map_err(ImageError::Unsupported)?;
        }
        Ok(())
    }
}

/// `data`, a `kind` picture at `path`, as the model is shown it: decoded, and
/// scaled to fit `max_dimension` when it is longer.
///
/// # Errors
/// The picture does not decode as `kind`, or the scaled picture could not
/// be written.
fn fitted(
    path: &str,
    kind: ImageKind,
    data: Bytes,
    max_dimension: u32,
) -> Result<Shown, ToolOutput> {
    let undecodable = |_undecodable: ImageError| {
        ToolOutput::failed(
            code::NOT_AN_IMAGE,
            &format!("{path} does not decode as a {} image", named(kind)),
        )
    };
    let mut decoder = ImageReader::with_format(Cursor::new(data.as_ref()), format_of(kind))
        .into_decoder()
        .map_err(undecodable)?;
    let metadata = Metadata {
        icc_profile: decoder
            .icc_profile()
            .ok()
            .flatten()
            .filter(|profile| profile.get(ICC_COLOR_SPACE) == Some(ICC_RGB)),
        exif: decoder.exif_metadata().ok().flatten(),
    };
    let picture = DynamicImage::from_decoder(decoder).map_err(undecodable)?;
    let from = (picture.width(), picture.height());
    if from.0 <= max_dimension && from.1 <= max_dimension {
        return Ok(Shown {
            image: Image { kind, data },
            scaled: None,
        });
    }
    let resized = picture.resize(max_dimension, max_dimension, FilterType::Triangle);
    let scaled = Scaled {
        from,
        to: (resized.width(), resized.height()),
    };
    let kind = written_as(kind);
    let data = rewritten(path, kind, scaled, encoded(&resized, kind, metadata))?;
    Ok(Shown {
        image: Image { kind, data },
        scaled: Some(scaled),
    })
}

/// The bytes `written`, the picture at `path` scaled by `scaled` and written
/// again as `kind`; a write that failed is refused naming both.
fn rewritten(
    path: &str,
    kind: ImageKind,
    scaled: Scaled,
    written: Result<Vec<u8>, ImageError>,
) -> Result<Bytes, ToolOutput> {
    match written {
        Ok(data) => Ok(Bytes::from(data)),
        Err(_unwritten) => Err(ToolOutput::failed(
            code::NOT_AN_IMAGE,
            &format!(
                "{path} could not be written again as a {} image once scaled from {scaled}",
                named(kind)
            ),
        )),
    }
}

/// `image`, written as `kind`: a JPEG at `JPEG_QUALITY`, a WebP lossless,
/// a PNG as eight-bit RGBA.
fn encoded(
    image: &DynamicImage,
    kind: ImageKind,
    metadata: Metadata,
) -> Result<Vec<u8>, ImageError> {
    let mut buffer = Vec::new();
    match kind {
        ImageKind::Jpeg => {
            let mut encoder = JpegEncoder::new_with_quality(&mut buffer, JPEG_QUALITY);
            metadata.apply(&mut encoder)?;
            encoder.encode_image(image)?;
        }
        ImageKind::Webp => {
            let rgba = image.to_rgba8();
            let mut encoder = WebPEncoder::new_lossless(&mut buffer);
            metadata.apply(&mut encoder)?;
            encoder.write_image(
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
                ExtendedColorType::Rgba8,
            )?;
        }
        ImageKind::Png | ImageKind::Gif => {
            let rgba = image.to_rgba8();
            let mut encoder = PngEncoder::new(&mut buffer);
            metadata.apply(&mut encoder)?;
            encoder.write_image(
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
                ExtendedColorType::Rgba8,
            )?;
        }
    }
    Ok(buffer)
}

/// The kind a scaled `kind` picture is written as: its own, but a GIF, which
/// decodes to its first frame and is written as a PNG.
const fn written_as(kind: ImageKind) -> ImageKind {
    match kind {
        ImageKind::Gif => ImageKind::Png,
        ImageKind::Png | ImageKind::Jpeg | ImageKind::Webp => kind,
    }
}

/// The decoder `kind` is read with.
const fn format_of(kind: ImageKind) -> ImageFormat {
    match kind {
        ImageKind::Png => ImageFormat::Png,
        ImageKind::Jpeg => ImageFormat::Jpeg,
        ImageKind::Gif => ImageFormat::Gif,
        ImageKind::Webp => ImageFormat::WebP,
    }
}

/// What a refusal calls `kind`.
const fn named(kind: ImageKind) -> &'static str {
    match kind {
        ImageKind::Png => "PNG",
        ImageKind::Jpeg => "JPEG",
        ImageKind::Gif => "GIF",
        ImageKind::Webp => "WebP",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Value: an encoder that refuses the scaled picture is the model's
    // refusal, naming the kind it was written as and the scaling, never a
    // picture sent half-written.
    #[test]
    fn a_scaled_picture_the_encoder_refuses_is_refused_naming_its_scaling() {
        let scaled = Scaled {
            from: (4096, 1024),
            to: (0, 0),
        };
        let none = Metadata {
            icc_profile: None,
            exif: None,
        };
        let refused = encoded(&DynamicImage::new_rgba8(0, 0), ImageKind::Jpeg, none);

        let shown = rewritten("wide.jpg", ImageKind::Jpeg, scaled, refused);

        assert_eq!(
            shown.err().map(|refusal| refusal.text),
            Some(
                "[not_an_image] wide.jpg could not be written again as a JPEG image once scaled \
                 from 4096x1024 to 0x0"
                    .to_owned()
            )
        );
    }
}
