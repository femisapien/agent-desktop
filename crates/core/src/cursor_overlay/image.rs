use crate::{AdapterError, ErrorCode, Point};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;

pub const MAX_CURSOR_IMAGE_PATH_BYTES: usize = 1024;
pub const MAX_CURSOR_IMAGE_BYTES: u64 = 2 * 1024 * 1024;
/// Largest PNG pixel width or height the renderer will decode.
pub const MAX_CURSOR_IMAGE_PIXELS: u32 = 8192;
const PNG_HEADER_BYTES: usize = 24;
const MAX_HOTSPOT: f64 = 512.0;
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
const PDF_MAGIC: &[u8] = b"%PDF-";

/// A PNG or PDF drawn in place of the built-in cursor arrow. `hotspot` is the
/// click point in image points from the image's top-left corner.
#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CursorImage {
    path: String,
    #[serde(default = "origin")]
    hotspot: Point,
}

impl CursorImage {
    pub fn new(path: String, hotspot: Point) -> Result<Self, AdapterError> {
        Self { path, hotspot }.validated()
    }

    pub fn validated(self) -> Result<Self, AdapterError> {
        if self.path.is_empty() || self.path.contains('\0') {
            return Err(invalid("Cursor image path must be a non-empty file path"));
        }
        if escaped_len(&self.path) > MAX_CURSOR_IMAGE_PATH_BYTES {
            return Err(invalid(format!(
                "Cursor image path must be at most {MAX_CURSOR_IMAGE_PATH_BYTES} bytes once JSON-escaped"
            )));
        }
        if !Path::new(&self.path).is_absolute() {
            return Err(invalid("Cursor image path must be absolute"));
        }
        if self.format().is_none() {
            return Err(invalid("Cursor image must be a .png or .pdf file"));
        }
        let within = |value: f64| value.is_finite() && (0.0..=MAX_HOTSPOT).contains(&value);
        if !within(self.hotspot.x) || !within(self.hotspot.y) {
            return Err(invalid(format!(
                "Cursor image --hotspot must be between 0 and {MAX_HOTSPOT} on both axes"
            )));
        }
        Ok(self)
    }

    /// Checks that the file is a regular file within the size limit, carries the signature
    /// its extension promises and, for a PNG, declares at most
    /// [`MAX_CURSOR_IMAGE_PIXELS`] per side. Every check reads the same open file.
    pub fn verify_file(&self) -> Result<(), AdapterError> {
        let unreadable = |error: std::io::Error| {
            invalid(format!("Cursor image '{}' cannot be read", self.path))
                .with_platform_detail(error.to_string())
        };
        let mut file = std::fs::File::open(&self.path).map_err(unreadable)?;
        let metadata = file.metadata().map_err(unreadable)?;
        if !metadata.is_file() {
            return Err(invalid(format!(
                "Cursor image '{}' is not a regular file",
                self.path
            )));
        }
        if metadata.len() > MAX_CURSOR_IMAGE_BYTES {
            return Err(invalid(format!(
                "Cursor image must be at most {} MiB",
                MAX_CURSOR_IMAGE_BYTES / (1024 * 1024)
            )));
        }
        let magic = self.format().unwrap_or(PNG_MAGIC);
        let header_len = if magic == PNG_MAGIC {
            PNG_HEADER_BYTES
        } else {
            magic.len()
        };
        let mut header = vec![0; header_len];
        file.read_exact(&mut header).map_err(unreadable)?;
        if !header.starts_with(magic) {
            return Err(invalid(format!(
                "Cursor image '{}' content does not match its extension",
                self.path
            )));
        }
        if magic == PNG_MAGIC && !png_dimensions_fit(&header) {
            return Err(invalid(format!(
                "Cursor image PNG must be at most {MAX_CURSOR_IMAGE_PIXELS} pixels per side"
            )));
        }
        Ok(())
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub const fn hotspot(&self) -> &Point {
        &self.hotspot
    }

    fn format(&self) -> Option<&'static [u8]> {
        let extension = Path::new(&self.path).extension()?.to_str()?;
        if extension.eq_ignore_ascii_case("png") {
            Some(PNG_MAGIC)
        } else if extension.eq_ignore_ascii_case("pdf") {
            Some(PDF_MAGIC)
        } else {
            None
        }
    }
}

/// Reads width and height from the IHDR chunk that follows the PNG signature.
fn png_dimensions_fit(header: &[u8]) -> bool {
    let side = |at: usize| {
        header
            .get(at..at + 4)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_be_bytes)
    };
    matches!(
        (header.get(12..16), side(16), side(20)),
        (Some(b"IHDR"), Some(width), Some(height))
            if (1..=MAX_CURSOR_IMAGE_PIXELS).contains(&width)
                && (1..=MAX_CURSOR_IMAGE_PIXELS).contains(&height)
    )
}

fn escaped_len(value: &str) -> usize {
    serde_json::to_string(value).map_or(usize::MAX, |json| json.len().saturating_sub(2))
}

const fn origin() -> Point {
    Point { x: 0.0, y: 0.0 }
}

fn invalid(message: impl Into<String>) -> AdapterError {
    AdapterError::new(ErrorCode::InvalidArgs, message)
}
