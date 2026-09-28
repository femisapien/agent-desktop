use crate::{AdapterError, ErrorCode, Point};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;

pub const MAX_CURSOR_IMAGE_PATH_BYTES: usize = 1024;
pub const MAX_CURSOR_IMAGE_BYTES: u64 = 2 * 1024 * 1024;
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
        if self.path.len() > MAX_CURSOR_IMAGE_PATH_BYTES {
            return Err(invalid(format!(
                "Cursor image path must be at most {MAX_CURSOR_IMAGE_PATH_BYTES} bytes"
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

    /// Checks that the file exists, fits the size limit, and carries the
    /// signature its extension promises.
    pub fn verify_file(&self) -> Result<(), AdapterError> {
        let metadata = std::fs::metadata(&self.path).map_err(|error| {
            invalid(format!("Cursor image '{}' cannot be read", self.path))
                .with_platform_detail(error.to_string())
        })?;
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
        let mut header = vec![0; magic.len()];
        std::fs::File::open(&self.path)
            .and_then(|mut file| file.read_exact(&mut header))
            .map_err(|error| {
                invalid(format!("Cursor image '{}' cannot be read", self.path))
                    .with_platform_detail(error.to_string())
            })?;
        if header != magic {
            return Err(invalid(format!(
                "Cursor image '{}' content does not match its extension",
                self.path
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

const fn origin() -> Point {
    Point { x: 0.0, y: 0.0 }
}

fn invalid(message: impl Into<String>) -> AdapterError {
    AdapterError::new(ErrorCode::InvalidArgs, message)
}
