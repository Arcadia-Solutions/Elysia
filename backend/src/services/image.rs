//! Pure image pipeline: decode, resize, encode, optional target-size search.
//! No DB, no HTTP: just bytes in, bytes out.

use image::metadata::Orientation;
use image::{DynamicImage, GenericImageView, ImageDecoder};
use serde::{Deserialize, Serialize};

/// Output format; `jxl` is accepted as an alias for `jpegxl` on input. Stored in
/// the DB as the `target_format` Postgres enum, mapped by the `sqlx::Type` derive.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, utoipa::ToSchema, sqlx::Type,
)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "target_format", rename_all = "lowercase")]
pub enum TargetFormat {
    Webp,
    #[serde(alias = "jxl")]
    Jpegxl,
    Avif,
    Png,
    #[serde(alias = "jpeg")]
    Jpg,
}

impl TargetFormat {
    /// Whether the format has a true lossless mode.
    pub fn supports_lossless(&self) -> bool {
        !matches!(self, TargetFormat::Avif | TargetFormat::Jpg)
    }

    fn ext_mime(&self) -> (&'static str, &'static str) {
        match self {
            TargetFormat::Webp => ("webp", "image/webp"),
            TargetFormat::Jpegxl => ("jxl", "image/jxl"),
            TargetFormat::Avif => ("avif", "image/avif"),
            TargetFormat::Png => ("png", "image/png"),
            TargetFormat::Jpg => ("jpg", "image/jpeg"),
        }
    }
}

pub struct ProcessOptions {
    pub target_width: u32,
    pub target_height: u32,
    pub format: TargetFormat,
    pub target_file_size: u64,
    pub requested_quality: Option<u8>,
}

pub struct Processed {
    pub bytes: Vec<u8>,
    pub ext: &'static str,
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
    pub applied_quality: Option<u8>,
}

#[derive(Debug)]
pub enum ProcessError {
    Undecodable,
    Encode(String),
}

/// A decoded image and its ICC profile, if any.
pub type Decoded = (DynamicImage, Option<Vec<u8>>);

/// `Ok(None)` means pass the source through untouched (animated GIF).
pub fn process(input: &[u8], options: &ProcessOptions) -> Result<Option<Processed>, ProcessError> {
    decode(input)?
        .map(|(image, icc_profile)| process_decoded(image, icc_profile.as_deref(), options))
        .transpose()
}

/// Decode input bytes to an image and its ICC profile. `Ok(None)` for
/// an animated GIF, which the caller passes through untouched. GIFs are decoded
/// only once: up to two frames are read and a still GIF reuses its single frame.
pub fn decode(input: &[u8]) -> Result<Option<Decoded>, ProcessError> {
    if input.starts_with(b"GIF") {
        return decode_gif(input);
    }
    let mut decoder = reader(input)
        .into_decoder()
        .map_err(|_| ProcessError::Undecodable)?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let icc_profile = decoder.icc_profile().ok().flatten();
    let mut image = DynamicImage::from_decoder(decoder).map_err(|_| ProcessError::Undecodable)?;
    // Re-encoding drops EXIF, so bake the orientation into the pixels.
    image.apply_orientation(orientation);
    Ok(Some((image, icc_profile)))
}

/// 64-bit DCT perceptual hash (pHash): the 8x8 lowest frequencies of the image
/// shrunk to 32x32 grayscale, each bit but the first set when above their
/// median. Visually similar images differ in few bits: compare with
/// `(a ^ b).count_ones()`.
pub fn perceptual_hash(image: &DynamicImage) -> i64 {
    const SIZE: usize = 32;
    const KEPT: usize = 8;
    let pixels = image.thumbnail_exact(SIZE as u32, SIZE as u32).to_luma32f();
    let cosines: [[f32; SIZE]; KEPT] = std::array::from_fn(|frequency| {
        std::array::from_fn(|position| {
            ((2 * position + 1) as f32 * frequency as f32 * std::f32::consts::PI
                / (2 * SIZE) as f32)
                .cos()
        })
    });
    // Separable 2D DCT-II, only for the kept frequencies: rows first, then columns.
    let mut row_frequencies = [[0f32; KEPT]; SIZE];
    for (row, frequencies) in row_frequencies.iter_mut().enumerate() {
        for (frequency, value) in frequencies.iter_mut().enumerate() {
            *value = (0..SIZE)
                .map(|column| {
                    pixels.get_pixel(column as u32, row as u32).0[0] * cosines[frequency][column]
                })
                .sum();
        }
    }
    let mut coefficients = [0f32; KEPT * KEPT];
    for (index, coefficient) in coefficients.iter_mut().enumerate() {
        let (vertical, horizontal) = (index / KEPT, index % KEPT);
        *coefficient = (0..SIZE)
            .map(|row| row_frequencies[row][horizontal] * cosines[vertical][row])
            .sum();
    }
    // The DC coefficient (index 0), the mean brightness, dwarfs the others: it
    // is left out of the median and its bit is never set.
    let mut sorted = coefficients;
    sorted[1..].sort_by(f32::total_cmp);
    let median = sorted[1 + (KEPT * KEPT - 1) / 2];
    coefficients
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, coefficient)| **coefficient > median)
        .fold(0u64, |hash, (index, _)| hash | 1 << index) as i64
}

/// `perceptual_hash` of encoded bytes, upright, the first frame of an animated
/// GIF. None when undecodable.
pub fn perceptual_hash_of(bytes: &[u8]) -> Option<i64> {
    let image = match decode(bytes).ok()? {
        Some((image, _)) => image,
        None => image::load_from_memory(bytes).ok()?,
    };
    Some(perceptual_hash(&image))
}

/// EXIF orientation of the file at `path`, read without a pixel decode.
/// `NoTransforms` when absent or unreadable.
pub fn orientation(path: &std::path::Path) -> Orientation {
    image::ImageReader::open(path)
        .and_then(|reader| reader.with_guessed_format())
        .map_err(image::ImageError::from)
        .and_then(|reader| reader.into_decoder())
        .and_then(|mut decoder| decoder.orientation())
        .unwrap_or(Orientation::NoTransforms)
}

fn reader(input: &[u8]) -> image::ImageReader<std::io::Cursor<&[u8]>> {
    image::ImageReader::new(std::io::Cursor::new(input))
        .with_guessed_format()
        .expect("reading from memory cannot fail")
}

/// Resize and encode an already-decoded image, embedding `icc_profile` where the
/// format allows. Lets callers decode once and reuse the pixels (e.g. a main
/// output plus a thumbnail from one source).
pub fn process_decoded(
    image: DynamicImage,
    icc_profile: Option<&[u8]>,
    options: &ProcessOptions,
) -> Result<Processed, ProcessError> {
    let image = resize(image, options.target_width, options.target_height);
    let (width, height) = image.dimensions();
    let rgba = image.to_rgba8();
    let (ext, mime) = options.format.ext_mime();

    // Explicit quality wins; else lossless, then optional target-size search.
    let (bytes, applied_quality) = match options.requested_quality {
        Some(quality) => {
            // PNG has no lossy mode; refuse rather than silently encode lossless.
            if matches!(options.format, TargetFormat::Png) {
                return Err(ProcessError::Encode(
                    "png has no lossy mode; set lossy_compression_value to 0".into(),
                ));
            }
            (
                encode(
                    &rgba,
                    width,
                    height,
                    &options.format,
                    icc_profile,
                    Some(quality),
                )?,
                Some(quality),
            )
        }
        None => {
            // AVIF and JPEG have no true lossless mode; refuse rather than
            // encode a near-lossless frame.
            if matches!(options.format, TargetFormat::Avif | TargetFormat::Jpg) {
                return Err(ProcessError::Encode(
                    "this image format has no lossless mode; set lossy_compression_value (1-100)"
                        .into(),
                ));
            }
            let lossless = encode(&rgba, width, height, &options.format, icc_profile, None)?;
            if options.target_file_size > 0 && lossless.len() as u64 > options.target_file_size {
                let (bytes, quality) = search_quality(
                    |quality| {
                        encode(
                            &rgba,
                            width,
                            height,
                            &options.format,
                            icc_profile,
                            Some(quality),
                        )
                    },
                    options.target_file_size,
                )?;
                (bytes, Some(quality))
            } else {
                (lossless, None)
            }
        }
    };

    Ok(Processed {
        bytes,
        ext,
        mime,
        width,
        height,
        applied_quality,
    })
}

/// Downscale to fit inside the target box, aspect preserved, never upscale.
/// A 0 on an axis means unconstrained.
fn resize(image: DynamicImage, target_width: u32, target_height: u32) -> DynamicImage {
    let (width, height) = image.dimensions();
    let max_width = if target_width == 0 {
        u32::MAX
    } else {
        target_width
    };
    let max_height = if target_height == 0 {
        u32::MAX
    } else {
        target_height
    };
    if width <= max_width && height <= max_height {
        return image;
    }
    image.resize(max_width, max_height, image::imageops::FilterType::Lanczos3)
}

/// Decode a GIF once: animated files pass through (`Ok(None)`), a still GIF
/// reuses its single frame instead of decoding a second time.
fn decode_gif(input: &[u8]) -> Result<Option<Decoded>, ProcessError> {
    use image::AnimationDecoder;
    use image::codecs::gif::GifDecoder;
    let decoder =
        GifDecoder::new(std::io::Cursor::new(input)).map_err(|_| ProcessError::Undecodable)?;
    let mut frames = decoder.into_frames().take(2);
    let first = frames
        .next()
        .transpose()
        .map_err(|_| ProcessError::Undecodable)?;
    let Some(first) = first else {
        return Err(ProcessError::Undecodable);
    };
    let second = frames
        .next()
        .transpose()
        .map_err(|_| ProcessError::Undecodable)?;
    if second.is_some() {
        return Ok(None);
    }
    Ok(Some((DynamicImage::ImageRgba8(first.into_buffer()), None)))
}

/// Encode RGBA pixels to the target format. `quality` None = lossless.
fn encode(
    rgba: &[u8],
    width: u32,
    height: u32,
    format: &TargetFormat,
    icc_profile: Option<&[u8]>,
    quality: Option<u8>,
) -> Result<Vec<u8>, ProcessError> {
    let bytes = match format {
        TargetFormat::Webp => Ok(encode_webp(rgba, width, height, quality)),
        TargetFormat::Avif => encode_avif(rgba, width, height, quality),
        TargetFormat::Jpegxl => encode_jxl(rgba, width, height, quality),
        TargetFormat::Png => encode_png(rgba, width, height),
        TargetFormat::Jpg => encode_jpeg(rgba, width, height, quality.unwrap_or(85)),
    }?;
    match icc_profile {
        // ponytail: ravif and jpegxl-rs expose no ICC setter, so AVIF and JPEG XL
        // are tagged sRGB and wide-gamut sources shift colour there. Convert the
        // pixels to sRGB (e.g. with qcms) if that matters.
        Some(icc_profile) if !matches!(format, TargetFormat::Avif | TargetFormat::Jpegxl) => {
            embed_icc_profile(bytes, icc_profile)
        }
        _ => Ok(bytes),
    }
}

/// Embed an ICC profile into encoded WebP, PNG or JPEG bytes (container rewrite).
fn embed_icc_profile(bytes: Vec<u8>, icc_profile: &[u8]) -> Result<Vec<u8>, ProcessError> {
    use img_parts::{DynImage, ImageICC};
    let mut image = DynImage::from_bytes(bytes.into())
        .map_err(|e| ProcessError::Encode(e.to_string()))?
        .ok_or_else(|| ProcessError::Encode("unknown container for ICC profile".into()))?;
    image.set_icc_profile(Some(icc_profile.to_vec().into()));
    let mut output = Vec::new();
    image
        .encoder()
        .write_to(&mut output)
        .map_err(|e| ProcessError::Encode(e.to_string()))?;
    Ok(output)
}

/// PNG encoder; always lossless, `quality` is ignored (PNG has no lossy mode).
fn encode_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, ProcessError> {
    use image::ImageEncoder;
    let mut buffer = Vec::new();
    image::codecs::png::PngEncoder::new(&mut buffer)
        .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
        .map_err(|e| ProcessError::Encode(e.to_string()))?;
    Ok(buffer)
}

/// JPEG encoder; lossy only. Alpha is dropped (JPEG has no alpha channel).
fn encode_jpeg(rgba: &[u8], width: u32, height: u32, quality: u8) -> Result<Vec<u8>, ProcessError> {
    use image::ImageEncoder;
    let rgb: Vec<u8> = rgba
        .chunks_exact(4)
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let mut buffer = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buffer, quality)
        .write_image(&rgb, width, height, image::ExtendedColorType::Rgb8)
        .map_err(|e| ProcessError::Encode(e.to_string()))?;
    Ok(buffer)
}

/// WebP encoder; `quality` None = lossless.
fn encode_webp(rgba: &[u8], width: u32, height: u32, quality: Option<u8>) -> Vec<u8> {
    let encoder = webp::Encoder::from_rgba(rgba, width, height);
    let memory = match quality {
        Some(quality) => encoder.encode(quality as f32),
        None => encoder.encode_lossless(),
    };
    memory.to_vec()
}

/// AVIF encoder. ravif is lossy-only; a `None` quality never reaches here
/// (lossless AVIF requests are rejected in `process`).
fn encode_avif(
    rgba: &[u8],
    width: u32,
    height: u32,
    quality: Option<u8>,
) -> Result<Vec<u8>, ProcessError> {
    use rgb::FromSlice;
    let pixels = ravif::Img::new(rgba.as_rgba(), width as usize, height as usize);
    let result = ravif::Encoder::new()
        .with_quality(quality.unwrap_or(100) as f32)
        .with_speed(6)
        .encode_rgba(pixels)
        .map_err(|e| ProcessError::Encode(e.to_string()))?;
    Ok(result.avif_file)
}

/// JPEG XL encoder; `quality` None = lossless (original profile, alpha kept).
fn encode_jxl(
    rgba: &[u8],
    width: u32,
    height: u32,
    quality: Option<u8>,
) -> Result<Vec<u8>, ProcessError> {
    let mut builder = jpegxl_rs::encoder_builder();
    builder.has_alpha(true);
    match quality {
        None => builder
            .lossless(true)
            .uses_original_profile(true)
            .quality(0.0),
        Some(quality) => builder
            .lossless(false)
            .quality(quality_to_distance(quality)),
    };
    let mut encoder = builder
        .build()
        .map_err(|e| ProcessError::Encode(e.to_string()))?;
    let result: jpegxl_rs::encode::EncoderResult<u8> = encoder
        .encode_frame(
            &jpegxl_rs::encode::EncoderFrame::new(rgba).num_channels(4),
            width,
            height,
        )
        .map_err(|e| ProcessError::Encode(e.to_string()))?;
    Ok(result.data)
}

/// Map a 0-100 quality to a libjxl distance (0 = lossless, ~15 = worst).
fn quality_to_distance(quality: u8) -> f32 {
    (100 - quality) as f32 * 0.15
}

/// Largest quality in 0..=100 whose encoded output fits under `target`, by
/// bisection (bounded steps). Falls back to the lowest-quality output.
fn search_quality(
    mut encode_at: impl FnMut(u8) -> Result<Vec<u8>, ProcessError>,
    target: u64,
) -> Result<(Vec<u8>, u8), ProcessError> {
    let (mut low, mut high) = (0u8, 100u8);
    let mut best = encode_at(low)?;
    let mut best_quality = low;
    for _ in 0..7 {
        let mid = low + (high - low) / 2;
        let candidate = encode_at(mid)?;
        if candidate.len() as u64 <= target {
            best = candidate;
            best_quality = mid;
            if mid == 100 {
                break;
            }
            low = mid + 1;
        } else {
            if mid == 0 {
                break;
            }
            high = mid - 1;
        }
        if low > high {
            break;
        }
    }
    Ok((best, best_quality))
}
