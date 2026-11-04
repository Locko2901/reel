//! A poster from an uploaded JPEG, PNG or WebP, sized from its header before anything decodes
//! it and encoded again by ffmpeg without its metadata.
use crate::Ctx;
use anyhow::Context as _;
use reel_store::models::{Clip, Rendition};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

/// The most an upload may weigh: a phone's photo, a screenshot, with room to spare.
pub const MAX_BYTES: usize = 25 * 1024 * 1024;
/// The most pixels it may have, so a small file that unpacks huge never gets decoded.
pub const MAX_PIXELS: u64 = 50_000_000;
/// The longest side it may have.
pub const MAX_SIDE: u32 = 16_384;
/// The kept copy's widest, and a poster's width, as a frame's is (`args::poster`).
const SOURCE_WIDTH: u32 = 2560;
const POSTER_WIDTH: u32 = 1280;

/// What the bytes are, by their first bytes alone.
pub fn sniff(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0xFF, 0xD8, 0xFF, ..] => Some("jpeg"),
        [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, ..] => Some("png"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("webp"),
        _ => None,
    }
}

/// Why an image was refused, in the words the clip page shows.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum Refused {
    #[error("that is not a JPEG, PNG or WebP image")]
    NotAnImage,
    #[error("the image is over {} MB", MAX_BYTES / 1024 / 1024)]
    TooLarge,
    #[error("the image is {0} by {1} pixels, over the {MAX_PIXELS} pixels reel takes")]
    TooManyPixels(u32, u32),
    #[error("the image could not be read")]
    Unreadable,
    #[error("another image for this clip is still being read; try again in a moment")]
    Busy,
}

/// The widest and highest size an image's headers name, read before anything decodes it, so an
/// image that names a small size first and a huge one after is weighed by the huge one.
pub fn header_size(kind: &str, b: &[u8]) -> Option<(u32, u32)> {
    let sizes = match kind {
        "png" => png_size(b).into_iter().collect(),
        "jpeg" => jpeg_sizes(b),
        "webp" => webp_sizes(b),
        _ => Vec::new(),
    };
    let w = sizes.iter().map(|s| s.0).max()?;
    let h = sizes.iter().map(|s| s.1).max()?;
    Some((w, h))
}

fn be16(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16::from_be_bytes(
        b.get(at..at + 2)?.try_into().ok()?,
    )))
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn le24(b: &[u8], at: usize) -> Option<u32> {
    let x = b.get(at..at + 3)?;
    Some(u32::from(x[0]) | (u32::from(x[1]) << 8) | (u32::from(x[2]) << 16))
}

fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// The signature, then IHDR, which a PNG must start with: its length, its name, the width and
/// the height.
fn png_size(b: &[u8]) -> Option<(u32, u32)> {
    (b.get(12..16)? == b"IHDR").then_some(())?;
    Some((be32(b, 16)?, be32(b, 20)?))
}

/// Every frame header's size, walking the segments by their lengths and the entropy-coded data
/// after each scan up to its next marker.
fn jpeg_sizes(b: &[u8]) -> Vec<(u32, u32)> {
    let mut sizes = Vec::new();
    let mut i = 2;
    while i + 1 < b.len() {
        if b[i] != 0xFF {
            break;
        }
        while b.get(i) == Some(&0xFF) {
            i += 1;
        }
        let Some(&code) = b.get(i) else { break };
        i += 1;
        match code {
            0x01 | 0xD0..=0xD8 => continue,
            0xD9 => break,
            _ => {}
        }
        let Some(len) = be16(b, i).map(|l| l as usize) else {
            break;
        };
        if len < 2 {
            break;
        }
        // SOF0 to SOF15, apart from DHT (C4), JPG (C8) and DAC (CC): precision, height, width
        if (0xC0..=0xCF).contains(&code) && !matches!(code, 0xC4 | 0xC8 | 0xCC) {
            if let (Some(h), Some(w)) = (be16(b, i + 3), be16(b, i + 5)) {
                sizes.push((w, h));
            }
        }
        i += len;
        if code == 0xDA {
            // the scan's data: on to the next marker that is not a stuffed 0xFF or a restart
            while i + 1 < b.len()
                && !(b[i] == 0xFF && b[i + 1] != 0 && !(0xD0..=0xD7).contains(&b[i + 1]))
            {
                i += 1;
            }
        }
    }
    sizes
}

/// The canvas (VP8X), each image (VP8 and VP8L) and each animation frame (ANMF, and the image
/// in it).
fn webp_sizes(b: &[u8]) -> Vec<(u32, u32)> {
    let mut sizes = Vec::new();
    webp_chunks(b, 12, &mut sizes, false);
    sizes
}

/// The chunks from `i` on; an animation frame's own are read one level down, never deeper.
fn webp_chunks(b: &[u8], mut i: usize, sizes: &mut Vec<(u32, u32)>, in_frame: bool) {
    while let (Some(name), Some(len)) = (b.get(i..i + 4), le32(b, i + 4)) {
        let at = i + 8;
        match name {
            b"VP8X" => {
                if let (Some(w), Some(h)) = (le24(b, at + 4), le24(b, at + 7)) {
                    sizes.push((w + 1, h + 1));
                }
            }
            b"VP8 " => {
                if b.get(at + 3..at + 6) == Some(&[0x9D, 0x01, 0x2A][..]) {
                    if let (Some(w), Some(h)) = (b.get(at + 6..at + 8), b.get(at + 8..at + 10)) {
                        let w = u32::from(u16::from_le_bytes([w[0], w[1]]) & 0x3FFF);
                        let h = u32::from(u16::from_le_bytes([h[0], h[1]]) & 0x3FFF);
                        sizes.push((w, h));
                    }
                }
            }
            b"VP8L" => {
                if let (Some(0x2F), Some(bits)) = (b.get(at), le32(b, at + 1)) {
                    sizes.push(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1));
                }
            }
            b"ANMF" if !in_frame => {
                if let (Some(w), Some(h)) = (le24(b, at + 6), le24(b, at + 9)) {
                    sizes.push((w + 1, h + 1));
                }
                let end = (at + len as usize).min(b.len());
                webp_chunks(&b[..end], at + 16, sizes, true);
            }
            _ => {}
        }
        let Some(next) = at.checked_add(len as usize + (len as usize & 1)) else {
            break;
        };
        i = next;
    }
}

/// The clips an image is being taken for: one at a time per clip, so two uploads never write
/// the same `poster-source.jpg`.
static TAKING: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

struct Taking(String);

impl Taking {
    fn start(clip_id: &str) -> Option<Self> {
        let mut busy = TAKING.lock().unwrap_or_else(|e| e.into_inner());
        busy.insert(clip_id.to_string())
            .then(|| Taking(clip_id.to_string()))
    }
}

impl Drop for Taking {
    fn drop(&mut self) {
        TAKING
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}

/// The uploaded bytes on disk for as long as ffmpeg reads them, removed however `take` ends:
/// an answer, an error, or the request dropped half way.
struct Raw(PathBuf);

impl Drop for Raw {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Whether an image this wide and high may be decoded.
pub fn fits(width: u32, height: u32) -> bool {
    width > 0
        && height > 0
        && width <= MAX_SIDE
        && height <= MAX_SIDE
        && u64::from(width) * u64::from(height) <= MAX_PIXELS
}

/// A poster's size for a version this wide and high: 1280 wide, its height even.
pub fn poster_size(width: i64, height: i64) -> (u32, u32) {
    if width <= 0 || height <= 0 {
        return (POSTER_WIDTH, 720);
    }
    let h = (f64::from(POSTER_WIDTH) * height as f64 / width as f64).round() as u32;
    (POSTER_WIDTH, h.max(2).div_ceil(2) * 2)
}

pub fn source_path(ctx: &Ctx, clip_id: &str) -> PathBuf {
    ctx.config().clip_dir(clip_id).join("poster-source.jpg")
}

fn args_source(input: &Path) -> Vec<String> {
    [
        // the decoder's own limit too, behind the header's
        "-max_pixels",
        &MAX_PIXELS.to_string(),
        "-i",
        &input.to_string_lossy(),
        "-map_metadata",
        "-1",
        "-frames:v",
        "1",
        "-vf",
        &format!("scale='min({SOURCE_WIDTH},iw)':-2:flags=lanczos,format=yuvj420p"),
        "-q:v",
        "2",
        "-f",
        "image2",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn args_poster(source: &Path, (w, h): (u32, u32)) -> Vec<String> {
    [
        "-i",
        &source.to_string_lossy(),
        "-map_metadata",
        "-1",
        "-frames:v",
        "1",
        "-vf",
        &format!("scale={w}:{h}:force_original_aspect_ratio=increase:flags=lanczos,crop={w}:{h},format=yuvj420p"),
        "-q:v",
        "3",
        "-f",
        "image2",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// The uploaded image becomes the clip's poster: checked, kept as `poster-source.jpg`, and the
/// current version's poster made from it. Answers the moment that keys the poster's address.
pub async fn take(ctx: &Ctx, clip: &Clip, bytes: &[u8]) -> anyhow::Result<Result<String, Refused>> {
    if bytes.len() > MAX_BYTES {
        return Ok(Err(Refused::TooLarge));
    }
    let Some(kind) = sniff(bytes) else {
        return Ok(Err(Refused::NotAnImage));
    };
    let Some((width, height)) = header_size(kind, bytes) else {
        return Ok(Err(Refused::Unreadable));
    };
    if !fits(width, height) {
        return Ok(Err(if width == 0 || height == 0 {
            Refused::Unreadable
        } else {
            Refused::TooManyPixels(width, height)
        }));
    }
    let Some(_taking) = Taking::start(&clip.id) else {
        return Ok(Err(Refused::Busy));
    };
    let current = match clip.current_id.as_deref() {
        Some(id) => ctx.store.rendition(id).await?,
        None => None,
    }
    .context("the clip has no current version")?;
    crate::library::ensure(ctx).await?;
    let dir = ctx.config().data_dir.join("cache").join("posters");
    tokio::fs::create_dir_all(&dir).await?;
    let raw = Raw(dir.join(format!("{}.{kind}", reel_core::ids::new_id())));
    tokio::fs::write(&raw.0, bytes).await?;
    let source = source_path(ctx, &clip.id);
    if ctx
        .ff
        .run_to(&args_source(&raw.0), &source, 0.0, None)
        .await
        .is_err()
    {
        return Ok(Err(Refused::Unreadable));
    }
    drop(raw);
    apply(ctx, clip, &current).await?;
    let at = reel_core::format::now();
    ctx.store.set_poster_image(&clip.id, &at).await?;
    tracing::info!(clip = %clip.id, width, height, kind, "an uploaded image is the poster");
    Ok(Ok(at))
}

/// A version's poster made from the clip's kept image, cropped to fill its frame, put in place
/// as a frame's poster is (written beside it, then renamed), its tint after it.
pub async fn apply(ctx: &Ctx, clip: &Clip, version: &Rendition) -> anyhow::Result<()> {
    let source = source_path(ctx, &clip.id);
    let video = ctx
        .config()
        .abs(version.path.as_deref().context("the version has no file")?);
    let size = poster_size(version.width.unwrap_or(0), version.height.unwrap_or(0));
    ctx.ff
        .run_to(
            &args_poster(&source, size),
            &crate::derived::paths(&video).poster,
            0.0,
            None,
        )
        .await
        .context("the poster from the uploaded image")?;
    crate::derived::tint(ctx, &video).await;
    Ok(())
}

/// After a version's poster was made from a frame, the clip's uploaded image, if it has one,
/// takes its place again.
pub async fn keep(ctx: &Ctx, clip_id: &str, version_id: &str) {
    let Ok(Some(clip)) = ctx.store.clip(clip_id).await else {
        return;
    };
    if clip.poster_image.is_none() || !source_path(ctx, clip_id).exists() {
        return;
    }
    let Ok(Some(version)) = ctx.store.rendition(version_id).await else {
        return;
    };
    if let Err(e) = apply(ctx, &clip, &version).await {
        tracing::warn!(clip = %clip_id, error = %e, "the uploaded poster for a version");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_image_is_known_by_its_bytes_not_its_name() {
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10]), Some("jpeg"));
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"), Some("png"));
        assert_eq!(sniff(b"RIFF\x24\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(sniff(b"GIF89a"), None);
        assert_eq!(sniff(b"<svg xmlns="), None);
        assert_eq!(sniff(b"\0\0\0\x18ftypheic"), None);
        assert_eq!(sniff(b"RIFF\x24\0\0\0WAVEfmt "), None);
        assert_eq!(sniff(&[0xFF, 0xD8]), None);
    }

    #[test]
    fn a_huge_image_is_never_decoded() {
        assert!(fits(4032, 3024), "a phone's 12 MP photo");
        assert!(fits(7680, 4320));
        assert!(!fits(10_000, 10_000), "100 MP");
        assert!(!fits(20_000, 100), "a side over 16384");
        assert!(!fits(0, 100));
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        b.extend(w.to_be_bytes());
        b.extend(h.to_be_bytes());
        b.extend([8, 2, 0, 0, 0]);
        b
    }

    fn sof(code: u8, w: u16, h: u16) -> Vec<u8> {
        let mut b = vec![0xFF, code, 0, 11, 8];
        b.extend(h.to_be_bytes());
        b.extend(w.to_be_bytes());
        b.extend([1, 1, 0x11, 0]);
        b
    }

    #[test]
    fn the_size_comes_from_the_header() {
        assert_eq!(
            header_size("png", &png(16_000, 16_000)),
            Some((16_000, 16_000))
        );
        assert!(
            !fits(16_000, 16_000),
            "the PNG bomb is refused before any decoder runs"
        );
        assert_eq!(header_size("png", b"\x89PNG\r\n\x1a\n"), None, "no IHDR");

        // SOI, an APP0 with a JPEG-looking byte pair inside it, a frame header, a scan whose
        // data holds a stuffed 0xFF, then a second frame header naming a huge size
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 8, b'J', b'F', 0xFF, 0xC0, 0, 0];
        jpeg.extend(sof(0xC0, 640, 480));
        jpeg.extend([
            0xFF, 0xDA, 0, 8, 1, 1, 0, 0, 0x3F, 0, 0x12, 0xFF, 0x00, 0x34, 0xFF, 0xD0, 0x56,
        ]);
        assert_eq!(header_size("jpeg", &jpeg), Some((640, 480)));
        jpeg.extend(sof(0xC2, 12_000, 9_000));
        jpeg.extend([0xFF, 0xD9]);
        assert_eq!(
            header_size("jpeg", &jpeg),
            Some((12_000, 9_000)),
            "the largest frame counts"
        );
        assert_eq!(header_size("jpeg", &[0xFF, 0xD8, 0xFF, 0xD9]), None);

        // RIFF, a VP8X canvas of 100 x 50, then a lossless image of 16000 x 3
        let mut webp = b"RIFF\0\0\0\0WEBPVP8X\x0a\0\0\0\0\0\0\0".to_vec();
        webp.extend([99, 0, 0, 49, 0, 0]);
        webp.extend(b"VP8L\x05\0\0\0\x2f");
        webp.extend((15_999u32 | (2 << 14)).to_le_bytes());
        webp.push(0);
        assert_eq!(header_size("webp", &webp), Some((16_000, 50)));
        let mut lossy = b"RIFF\0\0\0\0WEBPVP8 \x0a\0\0\0\0\0\0\x9d\x01\x2a".to_vec();
        lossy.extend([0x80, 0x07, 0x38, 0x04]);
        assert_eq!(header_size("webp", &lossy), Some((1920, 1080)));
        assert_eq!(header_size("webp", b"RIFF\0\0\0\0WEBP"), None);
    }

    #[test]
    fn one_image_at_a_time_per_clip() {
        let first = Taking::start("clip-a").expect("free");
        assert!(
            Taking::start("clip-a").is_none(),
            "a second upload waits its turn"
        );
        assert!(Taking::start("clip-b").is_some(), "another clip is free");
        drop(first);
        assert!(
            Taking::start("clip-a").is_some(),
            "free again once the first is done"
        );
    }

    #[test]
    fn the_decoder_is_held_to_the_limit_too() {
        let a = args_source(Path::new("/x/in.png"));
        let i = a.iter().position(|s| s == "-i").unwrap();
        assert_eq!(&a[..i], ["-max_pixels".to_string(), MAX_PIXELS.to_string()]);
    }

    #[test]
    fn a_poster_has_the_versions_shape() {
        assert_eq!(poster_size(1920, 1080), (1280, 720));
        assert_eq!(poster_size(1080, 1920), (1280, 2276));
        assert_eq!(poster_size(2560, 1080), (1280, 540));
        assert_eq!(poster_size(0, 0), (1280, 720));
    }
}
