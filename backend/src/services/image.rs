//! Pure image pipeline: decode, resize, encode, optional target-size search.
//! No DB, no HTTP: just bytes in, bytes out.

use image::{DynamicImage, GenericImageView};
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

/// `Ok(None)` means pass the source through untouched (animated GIF).
pub fn process(input: &[u8], options: &ProcessOptions) -> Result<Option<Processed>, ProcessError> {
    if is_animated_gif(input) {
        return Ok(None);
    }
    let image = image::load_from_memory(input).map_err(|_| ProcessError::Undecodable)?;
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
                    "png has no lossy mode; drop lossy_compression_value".into(),
                ));
            }
            (
                encode(&rgba, width, height, &options.format, Some(quality))?,
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
            let lossless = encode(&rgba, width, height, &options.format, None)?;
            if options.target_file_size > 0 && lossless.len() as u64 > options.target_file_size {
                let (bytes, quality) = search_quality(
                    |quality| encode(&rgba, width, height, &options.format, Some(quality)),
                    options.target_file_size,
                )?;
                (bytes, Some(quality))
            } else {
                (lossless, None)
            }
        }
    };

    Ok(Some(Processed {
        bytes,
        ext,
        mime,
        width,
        height,
        applied_quality,
    }))
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

/// True for a GIF with more than one frame; such files are passed through.
fn is_animated_gif(input: &[u8]) -> bool {
    use image::AnimationDecoder;
    use image::codecs::gif::GifDecoder;
    if !input.starts_with(b"GIF") {
        return false;
    }
    match GifDecoder::new(std::io::Cursor::new(input)) {
        Ok(decoder) => decoder.into_frames().take(2).count() > 1,
        Err(_) => false,
    }
}

/// Encode RGBA pixels to the target format. `quality` None = lossless.
fn encode(
    rgba: &[u8],
    width: u32,
    height: u32,
    format: &TargetFormat,
    quality: Option<u8>,
) -> Result<Vec<u8>, ProcessError> {
    match format {
        TargetFormat::Webp => Ok(encode_webp(rgba, width, height, quality)),
        TargetFormat::Avif => encode_avif(rgba, width, height, quality),
        TargetFormat::Jpegxl => encode_jxl(rgba, width, height, quality),
        TargetFormat::Png => encode_png(rgba, width, height),
        TargetFormat::Jpg => encode_jpeg(rgba, width, height, quality.unwrap_or(85)),
    }
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
